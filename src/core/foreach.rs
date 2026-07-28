//! `.foreach` — batch commands over matching container items (no-Lich),
//! modeled on foreach.lic 1.2.0 so muscle memory transfers:
//!
//! `.foreach [unique] [first N] [after N] [sorted] [reversed]
//!  [ATTR=]VALUE in CONTAINER[,CONTAINER...]; command; command...`
//!
//! - ATTR: `type` (default, gameobj-data tags), `sellable`, `noun`,
//!   `name`/`fullname`/`quick` (wildcard `*` text match; quick wraps the
//!   value in `*...*`), shorthands t/s/n/m/f/q; `/regex/` values match
//!   the name. Comma lists OR within one filter. `type=none` matches
//!   untyped items.
//! - Targets are tracked-container titles (same "blind to closed
//!   containers" caveat as foreach.lic); a trailing `?` makes a missing
//!   container skippable instead of an error.
//! - Commands substitute the words `item` -> `#<exist id>` and
//!   `container` -> `#<container id>` (ids work on direct connections;
//!   Lich just middle-mans game commands). Specials: `waitrt`,
//!   `sleep <s>`, `echo <text>`, `move to <container>` (a `_drag`).
//!   No commands at all = a dry run listing the matches.
//! - An implicit `get item` precedes a first command of
//!   drop/place/sell/appraise/trash/register/mark/unmark, like
//!   foreach.lic. (Its implicit RETURN-after-appraise is NOT mirrored —
//!   see the plan doc.)
//!
//! Execution runs under the automation lease as a root owner; the
//! service mirrors `TravelService` (tick + outbound queue drained by
//! frontends through the typed-command path). Pacing: one send per
//! tick, only while roundtime is clear, with a small gap after each.

use std::collections::VecDeque;
use std::time::Instant;

/// Gap after each sent command before the next (rough stand-in for
/// waiting out the game's response; foreach.lic waits on the prompt).
const INTER_COMMAND_MS: u64 = 300;

// ---------------------------------------------------------------------------
// Spec (parsed command line)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterAttr {
    Type,
    Sellable,
    Noun,
    Name,
}

pub struct ForeachSpec {
    pub unique: bool,
    pub first: Option<usize>,
    pub after: Option<usize>,
    pub sorted: bool,
    pub reversed: bool,
    pub attr: FilterAttr,
    /// Tag values for Type/Sellable (lowercased; "none" = untyped).
    pub tags: Vec<String>,
    /// Compiled patterns for Noun/Name.
    pub patterns: Vec<regex::Regex>,
    /// Container title queries; (query, optional) where optional = had `?`.
    pub targets: Vec<(String, bool)>,
    /// Raw command templates; empty = dry-run listing.
    pub commands: Vec<String>,
}

/// Convert a `*`-wildcard value to an anchored case-insensitive regex.
fn glob_to_regex(value: &str) -> Result<regex::Regex, String> {
    let mut pattern = String::from("^");
    for ch in value.chars() {
        if ch == '*' {
            pattern.push_str(".*");
        } else {
            pattern.push_str(&regex::escape(&ch.to_string()));
        }
    }
    pattern.push('$');
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| e.to_string())
}

pub fn parse(raw: &str) -> Result<ForeachSpec, String> {
    const USAGE: &str =
        "usage: .foreach [unique] [first N] [after N] [sorted] [reversed] \
         [attr=]value in <container>[,...][; command; ...]";

    let mut segments = raw.split(';');
    let head = segments.next().unwrap_or("").trim().to_string();
    let commands: Vec<String> = segments
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut spec = ForeachSpec {
        unique: false,
        first: None,
        after: None,
        sorted: false,
        reversed: false,
        attr: FilterAttr::Type,
        tags: Vec::new(),
        patterns: Vec::new(),
        targets: Vec::new(),
        commands,
    };

    // Options lead (foreach.lic order), then ONE value expression (may
    // contain spaces: `name=*quartz orb`), then the in/on/under/behind
    // keyword, then targets.
    let tokens: Vec<&str> = head.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].to_lowercase().as_str() {
            "unique" => spec.unique = true,
            "sorted" | "sort" | "nsorted" | "nsort" => spec.sorted = true,
            "reversed" | "reverse" => spec.reversed = true,
            kw @ ("first" | "after") => {
                i += 1;
                let n: usize = tokens
                    .get(i)
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| format!("'{}' needs a number. {}", kw, USAGE))?;
                if kw == "first" {
                    spec.first = Some(n);
                } else {
                    spec.after = Some(n);
                }
            }
            _ => break,
        }
        i += 1;
    }
    let rest = &tokens[i..];
    let prep = rest
        .iter()
        .position(|t| matches!(t.to_lowercase().as_str(), "in" | "on" | "under" | "behind"))
        .ok_or_else(|| format!("missing 'in <container>'. {}", USAGE))?;
    let filter_expr = rest[..prep].join(" ");
    let target_text = rest[prep + 1..].join(" ");

    if target_text.trim().is_empty() {
        return Err(format!("missing container after 'in'. {}", USAGE));
    }
    for part in target_text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (query, optional) = match part.strip_suffix('?') {
            Some(stripped) => (stripped.trim(), true),
            None => (part, false),
        };
        spec.targets.push((query.to_lowercase(), optional));
    }

    // The value expression: `attr=value`, a bare `/regex/`, or a bare
    // type value (`.foreach box in inv`). Empty = match everything.
    let (attr, value, quick) = if filter_expr.is_empty() {
        (FilterAttr::Type, String::new(), false)
    } else if filter_expr.len() > 2
        && filter_expr.starts_with('/')
        && filter_expr.ends_with('/')
    {
        (FilterAttr::Name, filter_expr.clone(), false)
    } else if let Some((attr, value)) = filter_expr.split_once('=') {
        let (attr, quick) = match attr.to_lowercase().as_str() {
            "type" | "t" => (FilterAttr::Type, false),
            "sellable" | "s" => (FilterAttr::Sellable, false),
            "noun" | "n" => (FilterAttr::Noun, false),
            "name" | "m" | "fullname" | "f" => (FilterAttr::Name, false),
            "quick" | "q" => (FilterAttr::Name, true),
            other => return Err(format!("unknown attribute '{}'. {}", other, USAGE)),
        };
        (attr, value.to_string(), quick)
    } else {
        (FilterAttr::Type, filter_expr.clone(), false)
    };

    spec.attr = attr;
    for piece in value.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match attr {
            FilterAttr::Type | FilterAttr::Sellable => {
                spec.tags.push(piece.to_lowercase());
            }
            FilterAttr::Noun | FilterAttr::Name => {
                let re = if piece.len() > 2 && piece.starts_with('/') && piece.ends_with('/') {
                    regex::RegexBuilder::new(&piece[1..piece.len() - 1])
                        .case_insensitive(true)
                        .build()
                        .map_err(|e| format!("bad regex: {}", e))?
                } else if quick {
                    glob_to_regex(&format!("*{}*", piece))?
                } else {
                    glob_to_regex(piece)?
                };
                spec.patterns.push(re);
            }
        }
    }

    Ok(spec)
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// One item eligible for matching: parsed link data plus its classifier
/// tags (computed by the caller, which owns the GameObjData).
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub noun: String,
    pub name: String,
    pub container_id: String,
    pub types: Vec<String>,
    pub sellable: Vec<String>,
}

impl ForeachSpec {
    fn value_matches(&self, candidate: &Candidate) -> bool {
        match self.attr {
            FilterAttr::Type | FilterAttr::Sellable => {
                if self.tags.is_empty() {
                    return true; // no filter = everything
                }
                let have = if self.attr == FilterAttr::Type {
                    &candidate.types
                } else {
                    &candidate.sellable
                };
                self.tags.iter().any(|want| {
                    if want == "none" {
                        have.is_empty()
                    } else {
                        have.iter().any(|t| t.eq_ignore_ascii_case(want))
                    }
                })
            }
            FilterAttr::Noun => self
                .patterns
                .iter()
                .any(|re| re.is_match(&candidate.noun)),
            FilterAttr::Name => self
                .patterns
                .iter()
                .any(|re| re.is_match(&candidate.name)),
        }
    }

    /// Filter, order, and slice candidates (foreach.lic semantics:
    /// container order, then sorted/reversed, then unique, then
    /// AFTER-skip, then FIRST-take).
    pub fn select<'a>(&self, all: &'a [Candidate]) -> Vec<&'a Candidate> {
        let mut picked: Vec<&Candidate> =
            all.iter().filter(|c| self.value_matches(c)).collect();
        if self.sorted {
            picked.sort_by_key(|c| sort_key(&c.name));
        }
        if self.reversed {
            picked.reverse();
        }
        if self.unique {
            let mut seen = std::collections::HashSet::new();
            picked.retain(|c| seen.insert(c.name.to_lowercase()));
        }
        if let Some(after) = self.after {
            picked.drain(..after.min(picked.len()));
        }
        if let Some(first) = self.first {
            picked.truncate(first);
        }
        picked
    }
}

/// Sort key ignoring leading articles/quantifiers, foreach.lic-style.
fn sort_key(name: &str) -> String {
    let lower = name.to_lowercase();
    for article in ["a ", "an ", "some ", "the "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.to_string();
        }
    }
    lower
}

// ---------------------------------------------------------------------------
// Command building
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Send(String),
    /// Explicit wait until roundtime is clear (sends already gate on RT;
    /// this also covers "stand here until RT ends" between non-RT steps).
    WaitRt,
    SleepMs(u64),
    Echo(String),
}

#[derive(Debug, Clone)]
pub struct WorkItem {
    pub id: String,
    pub name: String,
    pub steps: Vec<Step>,
}

/// Substitute the standalone words `item`/`container` with `#<id>` forms.
fn substitute(template: &str, item_id: &str, container_id: &str) -> String {
    static WORD: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let word = WORD.get_or_init(|| regex::Regex::new(r"\b(item|container)\b").unwrap());
    word.replace_all(template, |caps: &regex::Captures| match &caps[1] {
        "item" => format!("#{}", item_id),
        _ => format!("#{}", container_id),
    })
    .into_owned()
}

/// First-command verbs that get an implicit `get item` prepended.
const IMPLICIT_GET: &[&str] = &[
    "drop", "place", "sell", "appraise", "trash", "register", "mark", "unmark",
];

/// Build the step list for one item. `resolve_container` maps a title
/// query to a container id (for `move to <name>`).
pub fn build_steps(
    commands: &[String],
    item: &Candidate,
    resolve_container: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (idx, template) in commands.iter().enumerate() {
        let lower = template.to_lowercase();
        if idx == 0 {
            let verb = lower.split_whitespace().next().unwrap_or("");
            if IMPLICIT_GET.contains(&verb) {
                steps.push(Step::Send(format!("get #{}", item.id)));
            }
        }
        if lower == "waitrt" || lower == "waitrt?" {
            steps.push(Step::WaitRt);
        } else if let Some(rest) = lower.strip_prefix("sleep ") {
            let secs: f64 = rest
                .trim()
                .parse()
                .map_err(|_| format!("bad sleep duration '{}'", rest.trim()))?;
            steps.push(Step::SleepMs((secs * 1000.0) as u64));
        } else if let Some(rest) = template.strip_prefix("echo ") {
            steps.push(Step::Echo(rest.to_string()));
        } else if let Some(dest) = lower.strip_prefix("move to ") {
            let dest = dest.trim();
            let target_id = resolve_container(dest)
                .ok_or_else(|| format!("move to: no tracked container matches '{}'", dest))?;
            steps.push(Step::Send(format!("_drag #{} #{}", item.id, target_id)));
        } else {
            steps.push(Step::Send(substitute(
                template,
                &item.id,
                &item.container_id,
            )));
        }
    }
    Ok(steps)
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

pub struct ForeachContext {
    pub rt_remaining: f64,
    pub now_ms: u64,
    pub dead: bool,
}

pub enum ForeachEvent {
    Send(String),
    Status(String),
    Done { items: usize },
    Failed(String),
}

pub struct ForeachTask {
    /// Human description for the lease chain ("gem in backpack").
    pub desc: String,
    items: Vec<WorkItem>,
    item_idx: usize,
    step_idx: usize,
    wait_until_ms: Option<u64>,
    announced_item: bool,
}

impl ForeachTask {
    pub fn new(desc: String, items: Vec<WorkItem>) -> ForeachTask {
        ForeachTask {
            desc,
            items,
            item_idx: 0,
            step_idx: 0,
            wait_until_ms: None,
            announced_item: false,
        }
    }

    pub fn is_finished(events: &[ForeachEvent]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, ForeachEvent::Done { .. } | ForeachEvent::Failed(_)))
    }

    /// Advance: emits at most one `Send` per tick (pacing), any number of
    /// status lines. RT gates every send.
    pub fn tick(&mut self, ctx: &ForeachContext) -> Vec<ForeachEvent> {
        let mut events = Vec::new();
        if ctx.dead {
            events.push(ForeachEvent::Failed("you died - stopping.".to_string()));
            return events;
        }
        if let Some(until) = self.wait_until_ms {
            if ctx.now_ms < until {
                return events;
            }
            self.wait_until_ms = None;
        }
        loop {
            let Some(item) = self.items.get(self.item_idx) else {
                events.push(ForeachEvent::Done {
                    items: self.items.len(),
                });
                return events;
            };
            if !self.announced_item {
                self.announced_item = true;
                events.push(ForeachEvent::Status(format!(
                    "{}/{}: {}",
                    self.item_idx + 1,
                    self.items.len(),
                    item.name
                )));
            }
            let Some(step) = item.steps.get(self.step_idx) else {
                self.item_idx += 1;
                self.step_idx = 0;
                self.announced_item = false;
                continue;
            };
            match step {
                Step::Send(command) => {
                    if ctx.rt_remaining > 0.05 {
                        return events; // RT gate; retry next tick
                    }
                    events.push(ForeachEvent::Send(command.clone()));
                    self.step_idx += 1;
                    self.wait_until_ms = Some(ctx.now_ms + INTER_COMMAND_MS);
                    return events; // one send per tick
                }
                Step::WaitRt => {
                    if ctx.rt_remaining > 0.05 {
                        return events;
                    }
                    self.step_idx += 1;
                }
                Step::SleepMs(ms) => {
                    self.wait_until_ms = Some(ctx.now_ms + ms);
                    self.step_idx += 1;
                    return events;
                }
                Step::Echo(text) => {
                    events.push(ForeachEvent::Status(text.clone()));
                    self.step_idx += 1;
                }
            }
        }
    }
}

/// Service host, mirroring `TravelService`: one task, an outbound queue
/// frontends drain through the typed-command path.
pub struct ForeachService {
    task: Option<ForeachTask>,
    outbound: VecDeque<String>,
    epoch: Instant,
}

impl Default for ForeachService {
    fn default() -> Self {
        ForeachService {
            task: None,
            outbound: VecDeque::new(),
            epoch: Instant::now(),
        }
    }
}

impl ForeachService {
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    pub fn is_running(&self) -> bool {
        self.task.is_some()
    }

    pub fn task(&self) -> Option<&ForeachTask> {
        self.task.as_ref()
    }

    pub fn set_task(&mut self, task: ForeachTask) {
        self.task = Some(task);
    }

    /// Cancel the active run. Returns whether one was running.
    pub fn stop(&mut self) -> bool {
        self.outbound.clear();
        self.task.take().is_some()
    }

    pub fn tick(&mut self, ctx: &ForeachContext) -> Vec<ForeachEvent> {
        let Some(task) = self.task.as_mut() else {
            return Vec::new();
        };
        let events = task.tick(ctx);
        if ForeachTask::is_finished(&events) {
            self.task = None;
        }
        let mut surfaced = Vec::new();
        for event in events {
            match event {
                ForeachEvent::Send(command) => self.outbound.push_back(command),
                other => surfaced.push(other),
            }
        }
        surfaced
    }

    pub fn take_outbound(&mut self) -> Vec<String> {
        self.outbound.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, noun: &str, name: &str, types: &[&str]) -> Candidate {
        Candidate {
            id: id.to_string(),
            noun: noun.to_string(),
            name: name.to_string(),
            container_id: "c1".to_string(),
            types: types.iter().map(|s| s.to_string()).collect(),
            sellable: Vec::new(),
        }
    }

    #[test]
    fn parses_the_full_grammar() {
        let spec = parse(
            "unique first 3 after 1 sorted gem in backpack, red sack?; get item; sell item",
        )
        .unwrap();
        assert!(spec.unique && spec.sorted && !spec.reversed);
        assert_eq!(spec.first, Some(3));
        assert_eq!(spec.after, Some(1));
        assert_eq!(spec.attr, FilterAttr::Type);
        assert_eq!(spec.tags, vec!["gem"]);
        assert_eq!(
            spec.targets,
            vec![("backpack".to_string(), false), ("red sack".to_string(), true)]
        );
        assert_eq!(spec.commands, vec!["get item", "sell item"]);
    }

    #[test]
    fn parses_attr_forms_and_dry_run() {
        let spec = parse("q=elan*pack in cloak").unwrap();
        assert_eq!(spec.attr, FilterAttr::Name);
        assert!(spec.commands.is_empty(), "no commands = dry run");
        assert!(spec.patterns[0].is_match("an Elanthian Guilds voucher pack"));

        let spec = parse("name=*quartz orb in inv-sack").unwrap();
        assert!(spec.patterns[0].is_match("dim quartz orb"));
        assert!(!spec.patterns[0].is_match("quartz orb holder"));

        let spec = parse("/sea glass/ in bag").unwrap();
        assert!(spec.patterns[0].is_match("mist blue sea glass disk"));

        assert!(parse("gem backpack; sell item").is_err(), "missing 'in'");
        assert!(parse("bogus=x in bag").is_err());
    }

    #[test]
    fn select_applies_filter_order_and_slices() {
        let all = vec![
            candidate("1", "sapphire", "blue sapphire", &["gem", "valuable"]),
            candidate("2", "rock", "smooth rock", &["junk"]),
            candidate("3", "crystal", "quartz crystal", &["gem"]),
            candidate("4", "sapphire", "blue sapphire", &["gem", "valuable"]),
        ];
        let spec = parse("gem in bag").unwrap();
        let picked: Vec<&str> = spec.select(&all).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(picked, vec!["1", "3", "4"]);

        let spec = parse("unique sorted gem in bag").unwrap();
        let picked: Vec<&str> = spec.select(&all).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(picked, vec!["1", "3"]); // sorted: blue.. then quartz..

        let spec = parse("after 1 first 1 gem in bag").unwrap();
        let picked: Vec<&str> = spec.select(&all).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(picked, vec!["3"]);

        let spec = parse("type=none in bag").unwrap();
        assert!(spec.select(&all).is_empty());
    }

    #[test]
    fn builds_steps_with_substitution_and_implicit_get() {
        let item = candidate("42", "sapphire", "blue sapphire", &["gem"]);
        let commands = vec!["sell item".to_string()];
        let steps = build_steps(&commands, &item, |_| None).unwrap();
        assert_eq!(
            steps,
            vec![
                Step::Send("get #42".to_string()),
                Step::Send("sell #42".to_string()),
            ]
        );

        let commands = vec![
            "get item".to_string(),
            "put item in container".to_string(),
        ];
        let steps = build_steps(&commands, &item, |_| None).unwrap();
        assert_eq!(
            steps,
            vec![
                Step::Send("get #42".to_string()),
                Step::Send("put #42 in #c1".to_string()),
            ]
        );

        let commands = vec!["move to locker sack".to_string()];
        let steps = build_steps(&commands, &item, |q| {
            (q == "locker sack").then(|| "999".to_string())
        })
        .unwrap();
        assert_eq!(steps, vec![Step::Send("_drag #42 #999".to_string())]);

        let commands = vec!["sleep 1.5".to_string(), "waitrt".to_string()];
        let steps = build_steps(&commands, &item, |_| None).unwrap();
        assert_eq!(steps, vec![Step::SleepMs(1500), Step::WaitRt]);

        assert!(build_steps(&["move to gone".to_string()], &item, |_| None).is_err());
    }

    #[test]
    fn executor_paces_sends_and_respects_rt() {
        let item_a = WorkItem {
            id: "1".to_string(),
            name: "blue sapphire".to_string(),
            steps: vec![
                Step::Send("get #1".to_string()),
                Step::Send("sell #1".to_string()),
            ],
        };
        let mut service = ForeachService::default();
        service.set_task(ForeachTask::new("test".to_string(), vec![item_a]));

        let ctx = |rt: f64, now: u64| ForeachContext {
            rt_remaining: rt,
            now_ms: now,
            dead: false,
        };

        // RT blocks the first send (status still announces the item).
        let events = service.tick(&ctx(3.0, 0));
        assert!(matches!(&events[0], ForeachEvent::Status(s) if s.contains("blue sapphire")));
        assert!(service.take_outbound().is_empty());

        // RT clear: one send, then the inter-command gap blocks.
        service.tick(&ctx(0.0, 100));
        assert_eq!(service.take_outbound(), vec!["get #1".to_string()]);
        service.tick(&ctx(0.0, 150));
        assert!(service.take_outbound().is_empty(), "still inside the gap");

        // Gap elapsed: second send, then the task retires with Done.
        service.tick(&ctx(0.0, 500));
        assert_eq!(service.take_outbound(), vec!["sell #1".to_string()]);
        let events = service.tick(&ctx(0.0, 900));
        assert!(events
            .iter()
            .any(|e| matches!(e, ForeachEvent::Done { items: 1 })));
        assert!(!service.is_running());
    }

    #[test]
    fn executor_stops_on_death() {
        let mut service = ForeachService::default();
        service.set_task(ForeachTask::new(
            "test".to_string(),
            vec![WorkItem {
                id: "1".to_string(),
                name: "x".to_string(),
                steps: vec![Step::Send("get #1".to_string())],
            }],
        ));
        let events = service.tick(&ForeachContext {
            rt_remaining: 0.0,
            now_ms: 0,
            dead: true,
        });
        assert!(ForeachTask::is_finished(&events));
        assert!(!service.is_running());
    }
}

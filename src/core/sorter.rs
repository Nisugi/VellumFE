//! `.sorter` — categorized container looks (sorter.lic's native cousin).
//!
//! When enabled, a main-stream "In the backpack you see a, b, c." line is
//! replaced by a header plus one line per item category:
//!
//! ```text
//! In the backpack:
//!   gem (3): blue sapphire (2), quartz crystal
//!   other (1): copper lockpick
//! ```
//!
//! Unlike sorter.lic (which reads Lich's GameObj container registry),
//! this transform is pure over the line's own segments — the look output
//! already carries every item as an `<a exist noun>` link — so there is
//! no cache-freshness dependency. Item links are preserved (each unique
//! name keeps its first occurrence's link and styling); duplicates
//! collapse to a count; categories appear in first-seen order with items
//! sorted by their last word, sorter.lic-style. Category labels render
//! monsterbold like sorter.lic's.
//!
//! The message processor swaps the original line for `lines[0]` and
//! re-feeds the rest through the normal flush path, so highlights and
//! squelches apply to the generated lines individually.

use crate::core::gameobj_data::GameObjData;
use crate::data::widget::{SpanType, TextSegment};

/// True when this main-stream line is a container look worth transforming.
/// Mirrors sorter.lic's guards, including skipping lines that describe
/// both an "In" and an "On" surface at once. Public so the flush path can
/// gate cheaply before touching the classifier.
pub fn is_container_look(full_text: &str) -> bool {
    let sees = full_text.contains(" you see ");
    let in_form = full_text.starts_with("In the ") && sees;
    let on_form = full_text.starts_with("On the ") && sees;
    let peering = full_text.starts_with("Peering into ") && full_text.contains("you see ");
    if !(in_form || on_form || peering) {
        return false;
    }
    !(full_text.contains("In the ") && full_text.contains("On the "))
}

struct Bucket {
    category: String,
    items: Vec<Entry>,
}

struct Entry {
    /// Lowercased name for dedup/sort.
    key: String,
    count: usize,
    /// First occurrence's segment, link and styling intact.
    segment: TextSegment,
}

/// Build the replacement lines for a container-look line, or None when
/// the line isn't one (or has no item links to categorize).
/// Extract (container_id, container_noun, items) from a main-stream
/// container-look line's segments — the container is the first `<a>` link,
/// its contents are the rest. This lets the GameObjects registry ingest
/// from the *visible look* (main stream), not only from `<inv>` paired
/// tags (the inventory-window feed) — needed because a plain `look in`
/// (and especially with Lich's `;sorter` reformatting) may deliver
/// contents only as this prose line. `None` when the line isn't a
/// container look or carries no item links.
pub fn extract_container_items(
    segments: &[TextSegment],
    full_text: &str,
) -> Option<(String, Vec<crate::core::game_objects::GameItem>)> {
    if !is_container_look(full_text) {
        return None;
    }
    let mut links = segments.iter().filter(|seg| seg.link_data.is_some());
    let container = links.next()?;
    let container_id = container.link_data.as_ref()?.exist_id.clone();
    let items: Vec<crate::core::game_objects::GameItem> = links
        .filter_map(|seg| {
            let link = seg.link_data.as_ref()?;
            Some(crate::core::game_objects::GameItem::new(
                link.exist_id.clone(),
                link.noun.clone(),
                seg.text.trim().to_string(),
            ))
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    Some((container_id, items))
}

pub fn transform(
    segments: &[TextSegment],
    full_text: &str,
    data: &GameObjData,
) -> Option<Vec<Vec<TextSegment>>> {
    if !is_container_look(full_text) {
        return None;
    }

    // First link = the container; the rest are its items.
    let mut links = segments.iter().filter(|seg| seg.link_data.is_some());
    let container = links.next()?;
    let items: Vec<&TextSegment> = links.collect();
    if items.is_empty() {
        return None;
    }

    // Group into categories, first-seen order; dedupe by name with counts.
    let mut buckets: Vec<Bucket> = Vec::new();
    for item in items {
        let noun = item
            .link_data
            .as_ref()
            .map(|l| l.noun.as_str())
            .unwrap_or("");
        let category = data
            .classify(item.text.trim(), noun)
            .unwrap_or_else(|| "other".to_string());
        let bucket = match buckets.iter_mut().find(|b| b.category == category) {
            Some(bucket) => bucket,
            None => {
                buckets.push(Bucket {
                    category,
                    items: Vec::new(),
                });
                buckets.last_mut().expect("just pushed")
            }
        };
        let key = item.text.trim().to_lowercase();
        match bucket.items.iter_mut().find(|e| e.key == key) {
            Some(entry) => entry.count += 1,
            None => bucket.items.push(Entry {
                key,
                count: 1,
                segment: item.clone(),
            }),
        }
    }

    // Header: everything up to and including the container link, then ":".
    let mut header: Vec<TextSegment> = Vec::new();
    for segment in segments {
        let is_container = std::ptr::eq(segment, container);
        header.push(segment.clone());
        if is_container {
            break;
        }
    }
    header.push(TextSegment::plain(":"));

    let mut lines = vec![header];
    for bucket in &mut buckets {
        // sorter.lic sorts within a category by the name's last word.
        bucket
            .items
            .sort_by_key(|e| e.key.rsplit(' ').next().unwrap_or("").to_string());
        let total: usize = bucket.items.iter().map(|e| e.count).sum();
        let mut line = vec![TextSegment {
            text: format!("  {} ({}): ", bucket.category, total),
            span_type: SpanType::Monsterbold,
            ..Default::default()
        }];
        for (idx, entry) in bucket.items.iter().enumerate() {
            if idx > 0 {
                line.push(TextSegment::plain(", "));
            }
            line.push(entry.segment.clone());
            if entry.count > 1 {
                line.push(TextSegment::plain(format!(" ({})", entry.count)));
            }
        }
        lines.push(line);
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::widget::LinkData;

    fn link(text: &str, id: &str, noun: &str) -> TextSegment {
        TextSegment {
            text: text.to_string(),
            link_data: Some(LinkData {
                exist_id: id.to_string(),
                noun: noun.to_string(),
                text: text.to_string(),
                coord: None,
            }),
            ..Default::default()
        }
    }

    fn data() -> GameObjData {
        GameObjData::parse(
            r#"<data>
                 <type name="gem"><name>^(blue sapphire|quartz crystal)$</name></type>
                 <type name="junk"><name>\brock\b</name></type>
               </data>"#,
        )
    }

    fn look_line() -> (Vec<TextSegment>, String) {
        let segments = vec![
            TextSegment::plain("In the "),
            link("backpack", "77", "backpack"),
            TextSegment::plain(" you see a "),
            link("blue sapphire", "1", "sapphire"),
            TextSegment::plain(", a "),
            link("quartz crystal", "2", "crystal"),
            TextSegment::plain(", a "),
            link("blue sapphire", "3", "sapphire"),
            TextSegment::plain(" and a "),
            link("copper lockpick", "4", "lockpick"),
            TextSegment::plain("."),
        ];
        let text: String = segments.iter().map(|s| s.text.as_str()).collect();
        (segments, text)
    }

    #[test]
    fn extract_container_items_from_look_line() {
        let (segments, text) = look_line();
        let (id, items) = extract_container_items(&segments, &text).unwrap();
        // First link is the container (backpack), rest are items.
        assert_eq!(id, "77");
        // 4 item links in look_line() (sapphire, crystal, sapphire, lockpick).
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].id, "1");
        assert_eq!(items[0].name, "blue sapphire");
        assert_eq!(items[3].name, "copper lockpick");
        // A non-look line yields nothing.
        assert!(extract_container_items(
            &[TextSegment::plain("You pick up a rock.")],
            "You pick up a rock."
        )
        .is_none());
    }

    #[test]
    fn categorizes_counts_and_keeps_links() {
        let (segments, text) = look_line();
        let lines = transform(&segments, &text, &data()).unwrap();

        // Header keeps the container link and drops the item list.
        let header: String = lines[0].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(header, "In the backpack:");
        assert!(lines[0][1].link_data.is_some());

        // gem bucket first (first-seen); duplicates counted; links kept;
        // sorted by last word ("crystal" < "sapphire"), sorter.lic-style.
        let gems: String = lines[1].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(gems, "  gem (3): quartz crystal, blue sapphire (2)");
        assert_eq!(lines[1][0].span_type, SpanType::Monsterbold);
        assert!(lines[1][1].link_data.is_some(), "item keeps its link");

        // Unclassified items land in "other".
        let other: String = lines[2].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(other, "  other (1): copper lockpick");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn ignores_non_look_lines_and_dual_surface() {
        let data = data();
        let plain = [TextSegment::plain("You pick up a rock.")];
        assert!(transform(&plain, "You pick up a rock.", &data).is_none());

        // "In the X ... On the X ..." combined lines pass through.
        let (segments, _) = look_line();
        let text = "In the counter you see a rock. On the counter you see a rock.";
        assert!(transform(&segments, text, &data).is_none());

        // A look with no item links (empty container phrasing) passes.
        let empty = vec![
            TextSegment::plain("In the "),
            link("pouch", "9", "pouch"),
            TextSegment::plain(" you see nothing."),
        ];
        let text: String = empty.iter().map(|s| s.text.as_str()).collect();
        assert!(transform(&empty, &text, &data).is_none());
    }
}

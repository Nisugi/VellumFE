# Generates tests/fixtures/bench_highlights.toml - the frozen realistic
# highlight set for the parse benchmark.

entries = []  # (name, dict)

def add(name, pattern, **kw):
    e = {"pattern": pattern}
    e.update(kw)
    entries.append((name, e))

# --- 1. Player names from the corpus (hot, fire constantly) ---
players_hot = ["Sorrenn","Bojezmyrth","Skeval","Manndril","Vivina","Zaaldine",
    "Tweaked","Pangin","Alsthar","Winchester","Nickonai","Taelrak","Clairebie",
    "Kyshaa","Drakanious","Zhagen","Yakushi","Lirasah","Alendi","Ehria",
    "Haidhelmunduhir","Sunflowerr","Illumiada","Ayvain","Dyreknor","Madaraa",
    "Nadagast","Deddalus","Moonbeamz","Epos","Urlach","Noxs","Dinaden","Dativoy",
    "Maedoc","Starmizt","Kabosy","Lesorrel","Ardwen","Bakarus","Tijay","Tranquia",
    "Altheren","Wolfstarr","Chiyanna","Gilvren","Dravur","Raincail","Zimitar","Trevic"]
palette = ["#00ffff","#ffd700","#ff69b4","#7fff00","#ff8c00","#9370db","#00fa9a","#f08080"]
for i, p in enumerate(players_hot):
    add("player_" + p.lower(), p, fast_parse=True, fg=palette[i % len(palette)], category="Players")

# --- 2. Item/creature phrases seen in the corpus (hot) ---
corpus_items = ["pumpkin spice muffin","wicker wastebin","silver lockpick","falcon feather",
    "copper lockpick","finely woven sable frock","rose-marrow potion",
    "swirled steel bowl full of sticks","puckish snow spirit","erratic celestial spirit",
    "water-beaded cerulean pit viper","green-eyed pale grey kitten",
    "ruffled blue adolescent penguin","cheerful forest spirit","azure-webbed glowbark puppet",
    "silvery-faced hill caiverine","merry taiga spirit","golden energy","a wall of force",
    "Motes of","Wisps of"]
for i, it in enumerate(corpus_items):
    add("item_%02d" % i, it, fast_parse=True, fg="#c0c0c0" if i % 2 else "#e6e6fa", category="Items")

# Multi-literal fast_parse groups (the common user idiom)
add("disks", "disk|Disk", fast_parse=True, fg="#87cefa", category="Items")
add("spirit_group", "snow spirit|forest spirit|taiga spirit|celestial spirit", fast_parse=True, fg="#98fb98", category="Items")
add("casting_group", "gestures.|concentrates.|murmurs a simple|focuses on", fast_parse=True, fg="#dda0dd", category="Magic")

# --- 3. Dormant creature highlights (realistic: most patterns sleep) ---
adjectives = ["giant","greater","lesser","ancient","young","massive","dark","black",
    "frost","fire","storm","cave","war","dread","spectral","ghostly","rabid","feral"]
creatures = ["rat","kobold","goblin","hobgoblin","orc","troll","ogre","wolverine",
    "bear","lion","panther","spider","scorpion","serpent","wyvern","drake","golem",
    "wraith","zombie","skeleton","ghoul","banshee","shade","stalker","hound"]
count = 0
for a in adjectives:
    for c in creatures:
        if count >= 300:
            break
        add("creature_%s_%s" % (a, c), "a %s %s" % (a, c), fast_parse=True, fg="#ff4500", bold=True, category="Creatures")
        count += 1
    if count >= 300:
        break

# --- 4. Regex patterns (mix hot and dormant) ---
regexes = [
    ("rx_roundtime", r"Roundtime:?\s*\d+", "#ffff00", False),
    ("rx_you_verb", r"You (?:swing|thrust|gesture|channel|hurl|fire)", "#ff6347", False),
    ("rx_damage", r"\d+ points? of damage", "#ff0000", True),
    ("rx_lord_title", r"(?:Lord|Lady|High Lord) \w+", "#daa520", False),
    ("rx_status", r"(?:He|She|It) (?:is|appears) (?:stunned|prone|dead|kneeling)", "#adff2f", False),
    ("rx_says", "\\w+ says?, \"", "#87ceeb", False),
    ("rx_exclaims", "\\w+ exclaims?, \"", "#87ceeb", False),
    ("rx_exp", r"Exp(?:erience)? (?:to next level|gained)", "#00ff7f", False),
    ("rx_silvers", r"\d+ silvers", "#c0c0c0", False),
    ("rx_arrives", r"\w+ just arrived", "#f0e68c", False),
]
for name, pat, fg, bold in regexes:
    add(name, pat, fg=fg, bold=bold, category="Regex")
# dormant regexes
for i in range(40):
    add("rx_dormant_%02d" % i, r"a nonexistent %d beast with \d+ heads" % i, fg="#123456", category="Regex")

# --- 5. Entire-line highlights ---
add("line_alsosee", "You also see", fast_parse=True, color_entire_line=True, fg="#9ba2b2", category="Room")
add("line_deathflash", "was just struck down", fast_parse=True, color_entire_line=True, fg="#ffffff", bg="#8b0000", category="Deaths")
add("line_mind", "Your mind is", fast_parse=True, color_entire_line=True, fg="#b0c4de", category="Status")
for i in range(20):
    add("line_dormant_%02d" % i, "benchmark dormant line pattern %d" % i, fast_parse=True, color_entire_line=True, fg="#222222", category="Dormant")

# --- 6. Redirects (targets exist in the bench window set) ---
add("redir_casting", "murmurs a simple|gestures.", fast_parse=True, redirect_to="thoughts", redirect_mode="redirect_copy", category="Redirects")
add("redir_says", "says,", fast_parse=True, redirect_to="speech", redirect_mode="redirect_copy", category="Redirects")
add("redir_arrivals", "just arrived|just left", fast_parse=True, redirect_to="logons", redirect_mode="redirect_copy", category="Redirects")
add("redir_deaths", "was just struck down", fast_parse=True, redirect_to="death", redirect_mode="redirect_only", category="Redirects")
add("redir_rx_yells", "\\w+ yells?, \"[^\"]+\"", redirect_to="speech", redirect_mode="redirect_copy", category="Redirects")
for i in range(10):
    add("redir_dormant_%02d" % i, "dormant redirect trigger %d" % i, fast_parse=True, redirect_to="familiar", redirect_mode="redirect_copy", category="Redirects")

# --- 7. Squelches ---
add("squelch_refreshed", "You feel more refreshed", fast_parse=True, squelch=True, category="Squelch")
add("squelch_disk_follows", "disk follows you", fast_parse=True, squelch=True, category="Squelch")
for i in range(8):
    add("squelch_dormant_%02d" % i, "dormant squelch pattern %d" % i, fast_parse=True, squelch=True, category="Squelch")

# --- 8. silent_prompt ---
add("silent_mind", "Your mind is as clear as a bell", fast_parse=True, silent_prompt=True, category="Silent")
add("silent_enlightened", "You currently are enlightened", fast_parse=True, silent_prompt=True, category="Silent")
for i in range(6):
    add("silent_dormant_%02d" % i, "dormant silent pattern %d" % i, fast_parse=True, silent_prompt=True, category="Silent")

# --- 9. Replacements (immediate + capture groups + window-scoped) ---
add("repl_muffin", "pumpkin spice muffin", fast_parse=True, replace="MUFFIN", fg="#ffa500", category="Replace")
add("repl_rx_damage", r"(\d+) points of damage", replace="[$1 dmg]", fg="#ff0000", category="Replace")
add("repl_rx_roundtime", r"Roundtime: (\d+) sec", replace="RT $1s", category="Replace")
add("repl_window_scoped", "wicker wastebin", fast_parse=True, replace="BIN", window="main", category="Replace")
add("repl_window_scoped2", "falcon feather", fast_parse=True, replace="FEATHER", window="thoughts", category="Replace")
for i in range(10):
    add("repl_dormant_%02d" % i, "dormant replace source %d" % i, fast_parse=True, replace="dormant target %d" % i, category="Replace")

# --- 10. Stream-scoped ---
add("stream_thoughts_hot", "Kyshaa", fast_parse=True, stream="thoughts", fg="#ee82ee", category="Streams")
for i in range(7):
    add("stream_dormant_%02d" % i, "dormant stream pattern %d" % i, fast_parse=True, stream="thoughts", fg="#334455", category="Streams")

# --- 11. Sounds (bench drains pending_sounds; file need not exist) ---
add("sound_death", "was just struck down!", fast_parse=True, sound="alert.wav", sound_volume=0.8, category="Sounds")
add("sound_dormant", "dormant sound trigger", fast_parse=True, sound="ding.wav", category="Sounds")

# --- emit TOML ---
def toml_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'

lines_out = [
    "# Frozen realistic highlight set for tests/bench_parse.rs.",
    "# 505 patterns mined from a real session log plus dormant filler,",
    "# covering fast_parse literals, multi-literal groups, regexes,",
    "# entire-line, redirects (copy+only), squelch, silent_prompt,",
    "# replacements (incl. capture groups and window-scoped), stream-scoped",
    "# and sound triggers. DO NOT EDIT - changing this invalidates all",
    "# benchmark comparisons.",
    "",
]
for name, e in entries:
    lines_out.append("[%s]" % name)
    for k in sorted(e):
        v = e[k]
        if isinstance(v, bool):
            lines_out.append("%s = %s" % (k, "true" if v else "false"))
        elif isinstance(v, float):
            lines_out.append("%s = %s" % (k, v))
        else:
            lines_out.append("%s = %s" % (k, toml_str(v)))
    lines_out.append("")

with open("tests/fixtures/bench_highlights.toml", "w", encoding="utf-8", newline="\n") as f:
    f.write("\n".join(lines_out))

print("total patterns: %d" % len(entries))

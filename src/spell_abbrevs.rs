//! Spell name abbreviations for perception window
//!
//! This module provides a mapping of full spell names to abbreviated versions,
//! based on Profanity's spell abbreviation system. Used when `use_short_spell_names`
//! is enabled in perception window settings.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Static map of spell name → abbreviation
pub static SPELL_ABBREVIATIONS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::with_capacity(300);

    // Bard spells
    m.insert("Bravery", "Brvry");
    m.insert("Heroism", "Hrsm");
    m.insert("Fortitude", "Frtd");
    m.insert("Song of Valor", "SoV");
    m.insert("Song of Luck", "SoL");
    m.insert("Kai's Triumph Song", "KTS");
    m.insert("Song of Mirrors", "SoM");
    m.insert("Song of Sonic Disruption", "SSD");
    m.insert("Song of Peace", "SoP");
    m.insert("Song of Noise", "SoN");
    m.insert("Song of Power", "SoPw");
    m.insert("Song of Unravelling", "SoU");
    m.insert("Song of Depression", "SoD");
    m.insert("Song of Rage", "SoR");
    m.insert("Song of Tonis", "SoT");
    m.insert("Traveler's Song", "TvS");
    m.insert("Song of Renewal", "SoRn");
    m.insert("Lullabye", "Llby");
    m.insert("Lullaby", "Llby");
    m.insert("Singing Sword Song", "SSS");
    m.insert("Tremors", "Trmr");
    m.insert("Empathic Focus", "EFoc");
    m.insert("Sonic Shield Song", "SSSh");
    m.insert("Sonic Weapon Song", "SWS");
    m.insert("Rhythm", "Rhym");
    m.insert("Elemental Saturation", "ESat");
    m.insert("Mass Effect Song", "MES");
    m.insert("Elemental Empowerment", "EEmp");

    // Cleric spells
    m.insert("Divine Favor", "DivF");
    m.insert("Benediction", "Bndc");
    m.insert("Preservation", "Prsv");
    m.insert("Holy Bolt", "HBlt");
    m.insert("Bless", "Blss");
    m.insert("Chant of Protection", "ChPr");
    m.insert("Divine Wrath", "DvWr");
    m.insert("Heroic Resolve", "HRes");
    m.insert("Self-Control", "SlfC");
    m.insert("Warding Sphere", "WSph");
    m.insert("Well of Life", "WoL");
    m.insert("Sanctify", "Snct");
    m.insert("Raise Dead", "RsDd");
    m.insert("Prayer of Protection", "PoP");
    m.insert("Neutralize Curse", "NuCr");
    m.insert("Herb Production", "HrbP");
    m.insert("Food Production", "FdPr");
    m.insert("Spiritual Bulwark", "SpBl");
    m.insert("Censure", "Cnsr");
    m.insert("Minor Sanctuary", "MnSn");
    m.insert("Miracle", "Mrcl");
    m.insert("Transference", "Trnf");
    m.insert("Focused Calm", "FoCs");

    // Empath spells
    m.insert("Heal", "Heal");
    m.insert("Empathy", "Empy");
    m.insert("Empathic Link", "EmLk");
    m.insert("Empathic Assault", "EmAs");
    m.insert("Force of Will", "FoW");
    m.insert("Empathic Absorption", "EmAb");
    m.insert("Empathic Dispel", "EmDs");
    m.insert("Strength of Will", "StWl");
    m.insert("Adrenal Surge", "AdSg");
    m.insert("Vertigo", "Vrtg");
    m.insert("Aura of the Arkati", "AoAk");

    // Major Elemental spells
    m.insert("Sleep", "Slp");
    m.insert("Chromatic Circle", "ChrC");
    m.insert("Thurfel's Ward", "ThWd");
    m.insert("Temporal Reversion", "TmpR");
    m.insert("Hurl Boulder", "HBld");
    m.insert("Tremors", "Trms");
    m.insert("Cone of Elements", "CoE");
    m.insert("Meteor Swarm", "MtSw");
    m.insert("Elemental Disjunction", "EDsj");
    m.insert("Elemental Wave", "EWv");
    m.insert("Duplicate", "Dplc");
    m.insert("Mana Leech", "MnLc");
    m.insert("Mystic Impedance", "MsIm");
    m.insert("Elemental Deflection", "EDfl");
    m.insert("Elemental Focus", "EFcs");
    m.insert("Elemental Targeting", "ETrg");

    // Minor Elemental spells
    m.insert("Elemental Defense I", "ED1");
    m.insert("Elemental Defense II", "ED2");
    m.insert("Elemental Defense III", "ED3");
    m.insert("Prismatic Guard", "PrGd");
    m.insert("Elemental Barrier", "EBar");
    m.insert("Elemental Dispel", "EDsp");
    m.insert("Elemental Blade", "EBld");
    m.insert("Elemental Strike", "EStr");
    m.insert("Elemental Bias", "EBis");
    m.insert("Elemental Edge", "EEdg");
    m.insert("Piercing Gaze", "PGaz");
    m.insert("Elemental Detect", "EDtc");
    m.insert("Lock Pick Enhancement", "LPE");
    m.insert("Disarm Enhancement", "DsEn");
    m.insert("Elemental Enhancement", "EEnh");

    // Ranger spells
    m.insert("Natural Colors", "NtCl");
    m.insert("Foraging", "Frgn");
    m.insert("Camouflage", "Camo");
    m.insert("Resist Elements", "RsEl");
    m.insert("Phoen's Strength", "PhSt");
    m.insert("Self Control", "SlCt");
    m.insert("Barkskin", "Bark");
    m.insert("Nature's Fury", "NtFy");
    m.insert("Wall of Thorns", "WoTh");
    m.insert("Assume Aspect", "AsAs");
    m.insert("Sunburst", "Snbt");
    m.insert("Spike Thorn", "SpTh");
    m.insert("Mobility", "Mbty");
    m.insert("Mass Calm", "MsCl");
    m.insert("Call Swarm", "ClSw");
    m.insert("Tangle Weed", "TgWd");
    m.insert("Sounds", "Snds");
    m.insert("Nature's Touch", "NtTc");

    // Paladin spells
    m.insert("Mantle of Faith", "MoF");
    m.insert("Pious Trial", "PiTr");
    m.insert("Faith's Clarity", "FtCl");
    m.insert("Holy Blade", "HBld");
    m.insert("Divine Shield", "DvSh");
    m.insert("Divine Strike", "DvSt");
    m.insert("Zealot", "Zlot");
    m.insert("Condemn", "Cndm");
    m.insert("Aura of the Arkati", "AoA");
    m.insert("Patron's Blessing", "PaBl");
    m.insert("Champion's Might", "ChMt");
    m.insert("Holy Warrior", "HWar");
    m.insert("Judgment", "Jdgm");
    m.insert("Crusade", "Crsd");

    // Sorcerer spells
    m.insert("Cloak of Shadows", "CoSh");
    m.insert("Pain", "Pain");
    m.insert("Animate Dead", "AnDd");
    m.insert("Disease", "Diss");
    m.insert("Curse", "Curs");
    m.insert("Pestilence", "Pest");
    m.insert("Limb Disruption", "LbDs");
    m.insert("Evil Eye", "EvEy");
    m.insert("Torment", "Trmn");
    m.insert("Dark Catalyst", "DkCt");
    m.insert("Nightmare", "Nmre");
    m.insert("Bind", "Bind");
    m.insert("Blood Burst", "BdBr");
    m.insert("Demonic Pact", "DmPc");
    m.insert("Planar Shift", "PlSh");
    m.insert("Mana Disruption", "MnDs");
    m.insert("Phase", "Phse");
    m.insert("Sacrificial Offering", "SfOf");
    m.insert("Demon Summoning", "DmSm");
    m.insert("Malevolent Assault", "MlAs");

    // Wizard spells
    m.insert("Minor Shock", "MnSh");
    m.insert("Minor Water", "MnWt");
    m.insert("Minor Acid", "MnAc");
    m.insert("Minor Fire", "MnFr");
    m.insert("Minor Cold", "MnCd");
    m.insert("Call Lightning", "ClLt");
    m.insert("Major Shock", "MjSh");
    m.insert("Major Water", "MjWt");
    m.insert("Major Acid", "MjAc");
    m.insert("Major Fire", "MjFr");
    m.insert("Major Cold", "MjCd");
    m.insert("Immolation", "Imml");
    m.insert("Chain Lightning", "ChLt");
    m.insert("Implosion", "Impl");
    m.insert("Weapon Fire", "WpFr");
    m.insert("Familiar Gate", "FmGt");
    m.insert("Wizard's Shield", "WzSh");
    m.insert("Invisibility", "Invs");
    m.insert("Mass Invisibility", "MsIn");
    m.insert("Blur", "Blur");
    m.insert("Haste", "Hste");
    m.insert("Mass Blur", "MsBl");
    m.insert("Wizard's Eye", "WzEy");
    m.insert("Call Familiar", "ClFm");
    m.insert("Lesser Shroud", "LsSh");
    m.insert("Leviathan", "Lvth");

    // Major Spiritual spells
    m.insert("Spirit Warding I", "SpW1");
    m.insert("Spirit Warding II", "SpW2");
    m.insert("Spirit Defense", "SpDf");
    m.insert("Disease Resistance", "DsRs");
    m.insert("Poison Resistance", "PoRs");
    m.insert("Spirit Fog", "SpFg");
    m.insert("Spirit Shield", "SpSh");
    m.insert("Spirit Strike", "SpSt");
    m.insert("Spell Shield", "SpSl");
    m.insert("Bane", "Bane");
    m.insert("Interference", "Intf");
    m.insert("Blindness", "Blnd");
    m.insert("Confusion", "Cnfs");
    m.insert("Heroism", "Hero");
    m.insert("Calm", "Calm");
    m.insert("Frenzy", "Frnz");
    m.insert("Mass Blind", "MsBd");
    m.insert("Silence", "Slnc");
    m.insert("Symbol of Power", "SyPw");

    // Minor Spiritual spells
    m.insert("Spirit Warding I", "SwI");
    m.insert("Spirit Defense", "SpDf");
    m.insert("Spirit Guide", "SpGd");
    m.insert("Detect Invisibility", "DtIn");
    m.insert("Spirit Barrier", "SpBr");
    m.insert("Water Walking", "WtWk");
    m.insert("Untrammel", "Untm");
    m.insert("Locate Person", "LoPe");
    m.insert("Spirit Slayer", "SpSl");
    m.insert("Dispel Invisibility", "DsIn");
    m.insert("Mass Dispel Invisibility", "MSDI");
    m.insert("Spiritual Weapon", "SpWp");

    // Savant base spells
    m.insert("Mindward", "MnWd");
    m.insert("Thought Lash", "ThLs");
    m.insert("Mental Awareness", "MnAw");
    m.insert("Suggest", "Sgst");
    m.insert("Mind Jolt", "MnJt");
    m.insert("Telekinesis", "Tlkn");
    m.insert("Force Projection", "FPrj");
    m.insert("Mind Over Body", "MOB");
    m.insert("Mass Suggestion", "MsSg");
    m.insert("Focused Vision", "FoVs");
    m.insert("Psychic Barrier", "PsBr");
    m.insert("Mental Dispel", "MnDp");
    m.insert("Mental Focus", "MnFc");
    m.insert("Premonition", "Prmn");

    // Arcane spells
    m.insert("Mana Focus", "MnFc");
    m.insert("Arcane Barrier", "ArBr");
    m.insert("Arcane Decoy", "ArDc");
    m.insert("Magic Resistance", "MgRs");
    m.insert("Arcane Weapon", "ArWp");
    m.insert("Arcane Shield", "ArSh");
    m.insert("Arcane Blast", "ArBl");
    m.insert("Elemental Saturation", "ESat");
    m.insert("Combat Mastery", "CbMs");
    m.insert("Physical Enhancement", "PhEn");
    m.insert("Martial Prowess", "MaPr");
    m.insert("Mass Guard", "MsGd");

    // Common effects
    m.insert("Strength", "Strgth");
    m.insert("Constitution", "Const");
    m.insert("Dexterity", "Dxtrty");
    m.insert("Agility", "Aglty");
    m.insert("Discipline", "Dscpln");
    m.insert("Aura", "Aura");
    m.insert("Logic", "Lgc");
    m.insert("Intuition", "Inttn");
    m.insert("Wisdom", "Wsdm");
    m.insert("Influence", "Inflnc");

    // Spirit Abilities / Combat Maneuvers
    m.insert("Guard Stance", "GdSt");
    m.insert("Berserk", "Brsk");
    m.insert("Stance of the Mongoose", "SoMo");
    m.insert("Combat Focus", "CbFc");
    m.insert("Surge of Strength", "SoSt");
    m.insert("Weapon Bonding", "WpBd");
    m.insert("Kroderine Soul", "KrSl");
    m.insert("Martial Mastery", "MaMa");
    m.insert("Warrior's Resolve", "WaRe");
    m.insert("Perfect Strike", "PfSt");
    m.insert("Combat Prowess", "CmPr");
    m.insert("Iron Skin", "IrSk");
    m.insert("Toughness", "Tghn");
    m.insert("Armor Blessing", "ArBl");
    m.insert("Shield Blessing", "ShBl");

    // Rogue abilities
    m.insert("Shadow Dance", "ShDc");
    m.insert("Cutthroat", "Ctth");
    m.insert("Swarm", "Swrm");
    m.insert("Vanish", "Vnsh");
    m.insert("Divert", "Dvrt");
    m.insert("Shadow Mastery", "ShMa");
    m.insert("Silent Strike", "SlSt");
    m.insert("Side by Side", "SxS");

    // Monk abilities
    m.insert("Inner Focus", "InFc");
    m.insert("Surge of Vitality", "SoVt");
    m.insert("Perfect Self", "PfSf");
    m.insert("Combat Mastery", "CbMa");
    m.insert("Mystic Strike", "MySt");
    m.insert("Martial Focus", "MaFc");
    m.insert("Energy Absorption", "EnAb");

    // Profession-generic enhancements
    m.insert("Enhancive", "Enhv");
    m.insert("Enchant", "Ench");
    m.insert("Ensorcell", "Ensr");
    m.insert("Blessed", "Blsd");
    m.insert("Sanctified", "Sntf");
    m.insert("Flaring", "Flrg");

    // Society abilities
    m.insert("Sign of Courage", "SoCr");
    m.insert("Sign of Determination", "SoDt");
    m.insert("Sign of Smiting", "SoSm");
    m.insert("Sign of Swords", "SoSw");
    m.insert("Sign of Striking", "SoSt");
    m.insert("Sign of Defending", "SoDf");
    m.insert("Sign of Warding", "SoWa");
    m.insert("Sign of Shadows", "SoSh");
    m.insert("Sign of Thought", "SoTh");
    m.insert("Sign of Dissipation", "SoDs");
    m.insert("Sign of Healing", "SoHl");
    m.insert("Sign of Staunching", "SoSn");
    m.insert("Sign of Madness", "SoMd");
    m.insert("Sign of Turning", "SoTu");
    m.insert("Sign of Retribution", "SoRt");
    m.insert("Sign of Decay", "SoDc");
    m.insert("Sign of Darkness", "SoDk");
    m.insert("Sign of Unbinding", "SoUb");
    m.insert("Sign of Displacement", "SoDp");
    m.insert("Sign of Kneeling", "SoKn");
    m.insert("Sign of Probing", "SoPr");
    m.insert("Sign of Transcendence", "SoTr");
    m.insert("Sign of Life", "SoLf");
    m.insert("Sign of Supremacy", "SoSp");
    m.insert("Sign of Will", "SoWl");

    // Ascension abilities
    m.insert("Ascension", "Ascn");
    m.insert("Iron Stomach", "IrSt");
    m.insert("Second Wind", "ScWd");
    m.insert("Spirit Boost", "SpBt");
    m.insert("Mental Acuity", "MnAc");
    m.insert("Combat Clarity", "CbCl");
    m.insert("Critical Focus", "CrFc");
    m.insert("Crit Padding", "CrPd");

    // Misc/Common game effects
    m.insert("Haste", "Hst");
    m.insert("Slow", "Slw");
    m.insert("Stunned", "Stnd");
    m.insert("Webbed", "Wbbd");
    m.insert("Rooted", "Rtd");
    m.insert("Bound", "Bnd");
    m.insert("Calmed", "Clmd");
    m.insert("Silenced", "Slnd");
    m.insert("Blinded", "Bldd");
    m.insert("Feared", "Frd");
    m.insert("Confused", "Cnfd");
    m.insert("Prone", "Prne");
    m.insert("Kneeling", "Knlg");
    m.insert("Sitting", "Sttg");
    m.insert("Immobilized", "Immb");
    m.insert("Hidden", "Hddn");
    m.insert("Invisible", "Invs");
    m.insert("Flying", "Flyg");
    m.insert("Floating", "Fltg");

    m
});

/// Apply spell abbreviations to a string
///
/// Replaces all known full spell names with their abbreviated forms.
/// This function does a simple string replacement for each known spell.
pub fn abbreviate_spells(text: &str) -> String {
    let mut result = text.to_string();
    for (full, abbrev) in SPELL_ABBREVIATIONS.iter() {
        result = result.replace(full, abbrev);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_simple() {
        assert_eq!(abbreviate_spells("Bravery"), "Brvry");
        assert_eq!(abbreviate_spells("Heroism"), "Hrsm");
    }

    #[test]
    fn test_abbreviate_with_duration() {
        assert_eq!(abbreviate_spells("Bravery (94%)"), "Brvry (94%)");
        assert_eq!(abbreviate_spells("Song of Valor (OM)"), "SoV (OM)");
    }

    #[test]
    fn test_abbreviate_no_match() {
        assert_eq!(abbreviate_spells("Unknown Spell"), "Unknown Spell");
    }

    #[test]
    fn test_map_has_entries() {
        assert!(SPELL_ABBREVIATIONS.len() > 200);
    }
}

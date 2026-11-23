"""Which FM-1 sound a part plays, and its role when its track name does not say. The one table:
app/studio/engine/studio/src/sounds.rs is a copy of it (the Studio engine makes the same bytes; tests compare).

A part's sound comes from its text2midi instrument (named after "<role> – " in the track name, else by the whole
name, e.g. "Piano"; by id, name, alias), else from its General MIDI program, else none: the firmware plays the
role's default (sound 255), fit.py reports it. Sound ids are the firmware's catalog (synth.c, fixed): v4's 0-39,
SLOOP's synthesized kits 40-71, SLOOP's FM6 patches 72-79."""
from __future__ import annotations

# the catalog's names, by id (firmware/src/synth.c order; test 22.1 compares them with the firmware's own list)
NAMES = ("STD KIT", "TIGHT KIT", "BOOM KIT", "SAW LEAD", "SOFT PAD", "SQR BASS", "PWM STR", "ACID",
         "SINE KEY", "RAVE", "SUB BASS", "PLUCK", "BRASS", "WIND", "STRINGS", "BRASS",
         "ORGAN", "STRING", "RESO", "BELL", "WIRE", "FAT BASS", "ARP LEAD", "SYNC LEAD",
         "RING BELL", "CHIP CHOIR", "FULL ORGAN", "JAZZ PERC", "GOSPEL", "SOFT FLUTE", "ROCK DRIVE", "PULSE LD",
         "WAVE BASS", "ARP 8BIT", "WAVE LEAD", "STEP LEAD", "WIND", "RAIN", "ARCADE", "METAL",
         "808", "909", "606", "80S", "VINTAGE", "TRAP", "DRILL", "BOOMBAP",
         "LO-FI", "PHONK", "HOUSE", "D.HOUSE", "TECHNO", "MINIMAL", "ELECTRO", "DISCO",
         "GARAGE", "JUNGLE", "DUBSTEP", "DEMBOW", "AMAPIANO", "AFRO", "LATIN", "TRIBAL",
         "SYNTHWV", "CHIP", "ARCADE", "GLITCH", "INDUSTR", "HYPER", "AMBIENT", "JAZZ",
         "TINE EP", "GLASS BELL", "ROUND BASS", "BRASS SECT", "SOFT PAD", "WOOD BARS", "DRAWBARS", "NYLON PICK")
FM6_FIRST = 72
FM6_POOL = 3                                     # sloop_glue.c ENG_POOL_N: FM6 channels at once; more play FM6_ALT
FM6_ALT = (8, 19, 21, 12, 4, 24, 16, 11)         # FM6 72-79 -> the nearest sound of an engine without a pool

# General MIDI program (0-127) -> sound; one family of eight per line
GM = (8, 8, 8, 8, 72, 72, 11, 11,              # piano; e.pianos 4, 5; harpsichord, clavinet
      73, 73, 24, 77, 77, 77, 19, 11,          # celesta, glockenspiel, music box, vibes, marimba, xylo, bells, dulcimer
      78, 78, 26, 16, 28, 28, 13, 28,          # organs: drawbar, percussive, rock, church, reed; accordion, harmonica
      79, 79, 11, 11, 11, 30, 30, 24,          # guitars: nylon, steel, jazz, clean, muted, overdriven, distortion, harmonics
      74, 21, 21, 74, 5, 5, 10, 32,            # basses: acoustic, finger, pick, fretless, slap 1/2, synth 1/2
      17, 17, 17, 17, 6, 11, 11, 74,           # violin, viola, cello, contrabass, tremolo, pizzicato, harp, timpani
      14, 14, 6, 6, 25, 25, 25, 15,            # string ensembles, synth strings, choir, voice, synth voice, orchestra hit
      75, 75, 75, 12, 75, 75, 12, 15,          # trumpet, trombone, tuba, muted trumpet, horn, section, synth brass 1/2
      13, 13, 13, 13, 13, 13, 13, 29,          # saxes, oboe, english horn, bassoon; clarinet
      29, 29, 29, 29, 29, 29, 29, 29,          # piccolo, flute, recorder, pan flute, bottle, shakuhachi, whistle, ocarina
      31, 3, 29, 34, 30, 25, 23, 7,            # leads: square, saw, calliope, chiff, charang, voice, fifths, bass lead
      76, 4, 6, 25, 17, 20, 76, 18,            # pads: new age, warm, polysynth, choir, bowed, metallic, halo, sweep
      37, 4, 73, 4, 19, 38, 24, 38,            # synth fx: rain, soundtrack, crystal, atmosphere, brightness, goblins, echoes
      20, 11, 11, 79, 77, 13, 17, 13,          # sitar, banjo, shamisen, koto, kalimba, bagpipe, fiddle, shanai
      24, 27, 77, 27, 74, 74, 39, 36,          # tinkle bell, agogo, steel drums, woodblock, taiko, toms, reverse cymbal
      39, 36, 37, 38, 38, 36, 37, 39)          # fret noise, breath, seashore, bird, phone, helicopter, applause, gunshot
# the role of each family of eight programs, when the track name gives none
GM_ROLE = ("chords", "chords", "chords", "chords", "bass", "pad", "pad", "lead",
           "lead", "lead", "lead", "pad", "fx", "lead", "fx", "fx")
# a drum part's program -> kit: GM's kits are 0 standard, 8 room, 16 power, 24 electronic (25 TR-808), 32 jazz,
# 40 brush, 48 orchestra, 56 sound effects; the programs between are variations of the kit below them
DRUM_KITS = (0, 44, 2, 41, 71, 71, 0, 67)
TR808 = 25

# text2midi's instruments (t2m-core data/instruments, pinned in app/studio/engine): id -> (GM program, role)
INSTRUMENTS = {
    "keys.piano": (0, "chords"), "bass.electric": (33, "bass"), "kit.standard": (0, "drums"),
    "keys.rhodes": (4, "chords"), "pad.warm": (89, "pad"), "synth.lead": (80, "lead"), "bass.synth": (38, "bass"),
    "bass.upright": (32, "bass"), "guitar.clean": (27, "chords"), "guitar.overdrive": (29, "chords"),
    "guitar.nylon": (24, "chords"), "strings.ensemble": (48, "pad"), "harp.orchestral": (46, "arp"),
    "vibes.vibraphone": (11, "chords"), "fx.riser": (119, "fx"),
}
# their names and aliases, lower case -> id
ALIASES = {
    "acoustic grand piano": "keys.piano", "piano": "keys.piano", "grand piano": "keys.piano",
    "acoustic piano": "keys.piano",
    "electric bass (finger)": "bass.electric", "bass": "bass.electric", "electric bass": "bass.electric",
    "bass guitar": "bass.electric", "fingered bass": "bass.electric",
    "standard drum kit": "kit.standard", "drums": "kit.standard", "drum kit": "kit.standard", "kit": "kit.standard",
    "acoustic drums": "kit.standard",
    "electric piano": "keys.rhodes", "rhodes": "keys.rhodes", "e-piano": "keys.rhodes", "wurli": "keys.rhodes",
    "warm pad": "pad.warm", "synth pad": "pad.warm", "pad": "pad.warm",
    "square synth lead": "synth.lead", "synth lead": "synth.lead", "square lead": "synth.lead",
    "lead synth": "synth.lead", "synth": "synth.lead",
    "synth bass": "bass.synth", "808 bass": "bass.synth", "sub bass": "bass.synth",
    "acoustic bass": "bass.upright", "upright bass": "bass.upright", "double bass": "bass.upright",
    "standup bass": "bass.upright",
    "clean electric guitar": "guitar.clean", "clean guitar": "guitar.clean", "electric guitar": "guitar.clean",
    "guitar": "guitar.clean",
    "overdriven guitar": "guitar.overdrive", "distorted guitar": "guitar.overdrive", "rock guitar": "guitar.overdrive",
    "nylon-string guitar": "guitar.nylon", "nylon guitar": "guitar.nylon", "classical guitar": "guitar.nylon",
    "spanish guitar": "guitar.nylon",
    "string ensemble": "strings.ensemble", "strings": "strings.ensemble", "orchestral strings": "strings.ensemble",
    "string section": "strings.ensemble",
    "orchestral harp": "harp.orchestral", "harp": "harp.orchestral",
    "vibraphone": "vibes.vibraphone", "vibes": "vibes.vibraphone",
    "reverse cymbal riser": "fx.riser", "riser": "fx.riser", "sweep": "fx.riser", "reverse cymbal": "fx.riser",
}


def instrument(name: str) -> str | None:
    """the text2midi instrument a track name names: after "<role> – ", or the whole name; None when none"""
    s = name.split(" – ", 1)[-1].lower()
    return s if s in INSTRUMENTS else ALIASES.get(s)


def program_ok(program: int | None) -> bool:
    return program is not None and 0 <= program <= 127


def role_hint(name: str, program: int | None) -> str | None:
    """the role of a part whose name is not a role: its instrument's, else its program family's"""
    inst = instrument(name)
    if inst is not None:
        return INSTRUMENTS[inst][1]
    return GM_ROLE[program // 8] if program_ok(program) else None


def sound(name: str, program: int | None, role: str) -> int | None:
    """the sound a part plays; None when nothing names one (the firmware then plays the role's default)"""
    drums = role in ("drums", "percussion")
    inst = instrument(name)
    if inst is not None and (INSTRUMENTS[inst][1] == "drums") == drums:
        program = INSTRUMENTS[inst][0]
    elif not program_ok(program):
        return None
    if not drums:
        return GM[program]
    if program == TR808:
        return 40
    return DRUM_KITS[program // 8] if program < 64 else 0

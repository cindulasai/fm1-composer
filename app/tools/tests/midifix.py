"""Build small MIDI files for tests."""
import mido


def make_midi(path, parts, tpq=480, tempo=500000):
    """parts: [(track name, channel, [(tick, pitch, dur, vel), ...][, [mido messages at tick 0]]), ...]; names stored
    as UTF-8, as text2midi does"""
    m = mido.MidiFile(type=1, ticks_per_beat=tpq)
    cond = mido.MidiTrack([mido.MetaMessage("set_tempo", tempo=tempo, time=0)])
    m.tracks.append(cond)
    for name, ch, notes, *head in parts:
        evs = [(0, -1, msg) for msg in (head[0] if head else [])]
        for t, p, d, v in notes:
            evs.append((t, 1, mido.Message("note_on", channel=ch, note=p, velocity=v)))
            evs.append((t + d, 0, mido.Message("note_off", channel=ch, note=p, velocity=0)))
        evs.sort(key=lambda e: (e[0], e[1]))
        tr = mido.MidiTrack([mido.MetaMessage("track_name", name=name.encode("utf-8").decode("latin-1"), time=0)])
        last = 0
        for t, _, msg in evs:
            tr.append(msg.copy(time=t - last))
            last = t
        m.tracks.append(tr)
    m.save(str(path))
    return path

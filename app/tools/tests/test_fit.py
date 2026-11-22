"""US-3: fitting MIDI to the FM-1 (fit.py), every reduction reported."""
import sys
import pytest
from conftest import APP
from midifix import make_midi

sys.path.insert(0, str(APP / "tools"))
from fm1c.fit import FitError, Limits, fit, role_of  # noqa: E402
from fm1c.piece import decode, encode  # noqa: E402

ROLES10 = ["lead", "counter", "chords", "pad", "bass", "drums", "percussion", "arp", "drone", "fx"]


def ten_parts(tmp_path):
    parts = []
    for i, r in enumerate(ROLES10):
        ch = 9 if r == "drums" else i if i < 9 else 10
        parts.append((f"{r} – inst.{r}", ch, [(480 * k, 36 + i * 4, 240, 90) for k in range(8)]))
    return make_midi(tmp_path / "ten.mid", parts)


def test_3_1_ten_parts_become_eight_with_report(tmp_path):
    piece, rep = fit(str(ten_parts(tmp_path)))
    roles = [c.role for c in piece.channels]
    assert len(roles) == 8
    assert "percussion" not in roles                    # merged into the drums
    assert any("merged into the drums" in l for l in rep.lines)
    dropped = [l for l in rep.lines if "dropped: the FM-1 plays 8 channels" in l]
    assert len(dropped) == 1 and "fx" in dropped[0]
    assert roles[0] == "drums" and len(piece.channels[0].notes) == 16


def test_3_3_polyphony_cap_reported(tmp_path):
    chord = [(0, 48 + k, 960, 100 - k) for k in range(10)]
    f = make_midi(tmp_path / "c.mid", [("chords – keys", 0, chord)])
    piece, rep = fit(str(f), limits=Limits(poly=6))
    assert len(piece.channels[0].notes) == 6
    assert sorted(n.vel for n in piece.channels[0].notes) == [95, 96, 97, 98, 99, 100]   # the loudest stay
    assert any("4 notes dropped (more than 6 at once)" in l for l in rep.lines)


def test_3_3_range_fold_and_drums_untouched(tmp_path):
    f = make_midi(tmp_path / "r.mid", [("lead – x", 0, [(0, 12, 100, 80), (480, 110, 100, 80)]),
                                       ("drums – kit", 9, [(0, 20, 50, 100)])])
    piece, rep = fit(str(f))
    lead = next(c for c in piece.channels if c.role == "lead")
    assert sorted(n.pitch for n in lead.notes) == [24, 86]
    drums = next(c for c in piece.channels if c.role == "drums")
    assert drums.notes[0].pitch == 20
    assert any("lead: 2 notes moved by octaves" in l for l in rep.lines)


def test_3_3_ticks_scaled_to_96_ppq_and_tempo(tmp_path):
    f = make_midi(tmp_path / "t.mid", [("bass – b", 0, [(960, 40, 480, 70)])], tpq=480, tempo=600000)
    piece, rep = fit(str(f))
    n = piece.channels[0].notes[0]
    assert (n.tick, n.dur) == (192, 96) and piece.tempo_x10 == 1000
    assert piece.len_ticks == 384 and rep.reductions == 0
    assert decode(encode(piece)).channels[0].notes[0] == n


@pytest.mark.parametrize("name,src,role", [("lead – synth.lead", 0, "lead"), ("chords – keys.rhodes", 1, "chords"),
                                          ("Kick", 9, "drums"), ("My Strings", 2, "pad"), ("Track 7", 3, "other"),
                                          ("Counter melody", 4, "counter"), ("", 5, "other")])
def test_3_3_roles(name, src, role):
    assert role_of(name, src) == role


def test_3_4_no_notes_is_an_error(tmp_path):
    f = make_midi(tmp_path / "e.mid", [("lead", 0, [])])
    with pytest.raises(FitError, match="has no notes"):
        fit(str(f))


def test_3_4_tempo_clamped_and_reported(tmp_path):
    f = make_midi(tmp_path / "f.mid", [("lead", 0, [(0, 60, 10, 10)])], tempo=150000)   # 400 BPM
    piece, rep = fit(str(f))
    assert piece.tempo_x10 == 3000 and any("clamped to 300" in l for l in rep.lines)


def test_3_4_oversize_cut_at_a_bar(tmp_path):
    notes = [(k * 120, 40 + k % 30, 60, 90) for k in range(7000)]            # ~42 KB of events
    f = make_midi(tmp_path / "big.mid", [("lead", 0, notes)])
    piece, rep = fit(str(f), limits=Limits(poly=16))
    assert len(encode(piece)) <= 32768 and piece.len_ticks % 384 == 0
    assert any("cut to" in l and "32 KB" in l for l in rep.lines)


def test_3_1_unnamed_dropped_part_is_named_by_role(tmp_path):
    parts = [(f"{r}", 9 if r == "drums" else i, [(0, 50, 10, 90)]) for i, r in enumerate(ROLES10[:9])]
    parts.append(("", 10, [(0, 50, 10, 90)]))                    # no name, role "other": dropped last
    piece, rep = fit(str(make_midi(tmp_path / "n.mid", parts)))
    assert any("part 'other' (other, 1 notes) dropped" in l for l in rep.lines)


def test_3_3_range_edges_kept(tmp_path):
    f = make_midi(tmp_path / "e.mid", [("lead", 0, [(0, 24, 10, 80), (100, 96, 10, 80)])])
    piece, rep = fit(str(f))
    assert sorted(n.pitch for n in piece.channels[0].notes) == [24, 96] and rep.reductions == 0


def test_3_4_notes_past_999_bars_dropped(tmp_path):
    from fm1c.piece import MAX_TICKS
    f = make_midi(tmp_path / "long.mid", [("pad", 0, [(0, 60, 10, 80), (MAX_TICKS * 5, 62, 10, 80)])])
    piece, rep = fit(str(f))
    assert len(piece.channels[0].notes) == 1 and piece.len_ticks == MAX_TICKS
    assert any("notes past 999 bars dropped" in l for l in rep.lines)


def test_3_3_fold_lands_on_the_edge(tmp_path):
    f = make_midi(tmp_path / "e2.mid", [("lead", 0, [(0, 108, 10, 80), (100, 12, 10, 80)])])
    piece, _ = fit(str(f))
    assert sorted(n.pitch for n in piece.channels[0].notes) == [24, 96]


def test_3_3_note_lengths_survive_fitting(tmp_path):
    from fm1c import fit as fitmod
    mid = make_midi(tmp_path / "d.mid", [("lead", 0, [(960, 60, 480, 90), (1920, 62, 240, 90)])])   # 480 per beat
    piece, _ = fitmod.fit(str(mid))
    assert [(n.tick, n.dur) for n in piece.channels[0].notes] == [(192, 96), (384, 48)]


def test_3_3_a_note_never_released_ends_with_its_track(tmp_path):
    import mido
    from fm1c import fit as fitmod
    m = mido.MidiFile(type=1, ticks_per_beat=480)
    m.tracks.append(mido.MidiTrack([mido.MetaMessage("track_name", name="lead", time=0),
                                    mido.Message("note_on", note=60, velocity=90, time=960),      # never let go
                                    mido.Message("note_on", note=64, velocity=90, time=960),
                                    mido.Message("note_off", note=64, velocity=0, time=240)]))
    m.save(tmp_path / "hang.mid")
    piece, _ = fitmod.fit(str(tmp_path / "hang.mid"))
    assert sorted((n.tick, n.pitch, n.dur) for n in piece.channels[0].notes) == [(192, 60, 240), (384, 64, 48)]

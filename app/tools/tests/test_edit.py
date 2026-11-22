"""US-10: edit.c (the note editor's core) against a Python model: the C result must be byte for byte what
fm1c's encode() makes of the model's notes, valid after every edit, untouched by every refused one."""
import random, subprocess, sys
import pytest
from conftest import APP
from test_piece import rand_piece

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Piece, Channel, Note, encode, decode  # noqa: E402

OK, ARG, FULL, EXISTS, NONE = 0, 1, 2, 3, 4


@pytest.fixture(scope="module")
def runner(tmp_path_factory):
    exe = tmp_path_factory.mktemp("ed") / "edit_run"
    san = subprocess.run(["sh", str(APP / "tests/sanflags.sh")], capture_output=True, text=True, check=True).stdout.split()
    r = subprocess.run(["cc", "-O1", "-g", "-Wall", "-Werror", "-Wno-unused-function", *san, "-I", str(APP / "firmware/src"),
                        "-o", str(exe), str(APP / "tests/edit_run.c")], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr

    def run(lines):
        r = subprocess.run([str(exe)], input="\n".join(lines) + "\n", capture_output=True, text=True)
        assert r.returncode == 0 and "Sanitizer" not in r.stderr, r.stdout[-500:] + r.stderr
        assert "CHECK" not in r.stdout and "CHANGED" not in r.stdout, r.stdout
        return [int(x.split()[1]) for x in r.stdout.split("\n") if x.startswith("r ")]
    return run


class Model:
    """the editor's rules on a Piece; undo = the states before the last 64 changes"""
    def __init__(self, p):
        self.p, self.undo = p, []

    def _notes(self, ch):
        return self.p.channels[ch].notes

    def _find(self, ch, tick, pitch):
        for n in sorted(self._notes(ch)):
            if n.tick == tick and n.pitch == pitch:
                return n
        return None

    def _keep(self):
        self.undo = (self.undo + [[list(c.notes) for c in self.p.channels]])[-64:]

    def apply(self, op):
        k, a = op[0], op[1:]
        if k == "u":
            if not self.undo:
                return NONE
            for c, notes in zip(self.p.channels, self.undo.pop()):
                c.notes = notes
            return OK
        ch = a[0]
        if ch >= len(self.p.channels):
            return ARG
        if k == "i":
            _, tick, pitch, dur, vel = a
            if not (tick < self.p.len_ticks and pitch <= 127 and 1 <= dur <= 65535 and 1 <= vel <= 127):
                return ARG
            if self._find(ch, tick, pitch):
                return EXISTS
            self._keep()
            self._notes(ch).append(Note(tick, pitch, dur, vel))
            return OK
        if k == "s" and not (1 <= a[3] <= 65535 and 1 <= a[4] <= 127):
            return ARG
        n = self._find(ch, a[1], a[2]) if a[2] <= 127 else None
        if not n:
            return NONE
        self._keep()
        self._notes(ch).remove(n)
        if k == "s":
            self._notes(ch).append(Note(n.tick, n.pitch, a[3], a[4]))
        return OK


def line(op):
    return " ".join(map(str, op))


def distinct(p):
    """no two notes of a channel at the same tick and pitch (the editor names a note by them)"""
    for c in p.channels:
        seen = {}
        for n in c.notes:
            seen.setdefault((n.tick, n.pitch), n)
        c.notes = list(seen.values())
    return p


def rand_op(rng, m):
    p = m.p
    ch = rng.randrange(len(p.channels) + (1 if rng.random() < 0.05 else 0))
    notes = p.channels[ch].notes if ch < len(p.channels) else []
    k = rng.choice("iiiddsu")
    if k == "u":
        return ("u",)
    if notes and rng.random() < (0.8 if k != "i" else 0.15):
        n = rng.choice(notes)
        tick, pitch = n.tick, n.pitch
    else:
        tick = rng.randrange(p.len_ticks + 3)
        pitch = rng.randint(0, 129)
    dur = rng.choice([rng.randint(1, 400), 0, 65535, 65536, rng.randint(1, 65535)])
    vel = rng.choice([rng.randint(1, 127), 0, 128])
    return (k, ch, tick, pitch) + ((dur, vel) if k in "is" else ())


def check_case(runner, tmp_path, i, p, ops):
    src, out = tmp_path / f"p{i}.f1p", tmp_path / f"o{i}.f1p"
    src.write_bytes(encode(p))
    m = Model(decode(encode(p)))
    want = [m.apply(op) for op in ops]
    got = runner([f"load {src}", *map(line, ops), f"save {out}"])
    assert got == want, (i, ops)
    assert out.read_bytes() == encode(m.p), i
    return m


def test_10_2_random_edits_match_the_model(runner, tmp_path):
    rng = random.Random(102)
    lines, cases = [], []
    for i in range(1000):
        p = distinct(rand_piece(rng, nnotes=rng.randint(0, 40)))
        m = Model(decode(encode(p)))
        ops = []
        for _ in range(rng.randint(1, 25)):
            op = rand_op(rng, m)
            m.apply(op)
            ops.append(op)
        src, out = tmp_path / f"p{i}.f1p", tmp_path / f"o{i}.f1p"
        src.write_bytes(encode(p))
        m2 = Model(decode(encode(p)))
        cases.append(([m2.apply(op) for op in ops], out, m2))
        lines += [f"load {src}", *map(line, ops), f"save {out}"]
    got = runner(lines)
    want = [r for w, _, _ in cases for r in w]
    assert got == want
    assert sum(r == OK for r in want) > 5000 and {ARG, EXISTS, NONE} <= set(want)
    for w, out, m2 in cases:
        assert out.read_bytes() == encode(m2.p)


def eight():
    roles = ["lead", "bass", "drums", "pad", "chords", "counter", "arp", "fx"]
    return Piece("eight", 1200, 384 * 8, [Channel(r, [Note(t * 96, 50 + c, 48, 90) for t in range(32)]) for c, r in enumerate(roles)])


def test_10_1_insert_at_start_middle_and_last_tick(runner, tmp_path):
    p = eight()
    before = encode(p)
    m = check_case(runner, tmp_path, 0, p, [("i", 3, 0, 40, 10, 100), ("i", 3, 1000, 70, 10, 100), ("i", 3, 384 * 8 - 1, 60, 5, 1)])
    ch3 = sorted(m.p.channels[3].notes)
    assert ch3[0] == Note(0, 40, 10, 100) and ch3[-1] == Note(384 * 8 - 1, 60, 5, 1) and Note(1000, 70, 10, 100) in ch3
    after = encode(m.p)
    d0, d1 = decode(before), decode(after)
    assert [c.notes for i, c in enumerate(d0.channels) if i != 3] == [c.notes for i, c in enumerate(d1.channels) if i != 3]


def test_10_3_long_gaps_get_rests_and_lose_them(runner, tmp_path):
    p = Piece("gaps", 1200, 384 * 999, [Channel("lead", [Note(0, 60, 10, 90), Note(300000, 62, 10, 90)])])
    m = check_case(runner, tmp_path, 1, p, [("i", 0, 200000, 64, 10, 90)])
    raw = (tmp_path / "o1.f1p").read_bytes()
    assert raw.count(bytes([255, 255, 1, 0, 255, 1])) == 4          # 0 -> 200000: 3 rests, -> 300000: 1
    check_case(runner, tmp_path, 2, m.p, [("d", 0, 300000, 62)])
    raw = (tmp_path / "o2.f1p").read_bytes()
    assert raw.count(bytes([255, 255, 1, 0, 255, 1])) == 3          # the last gap's rest went with its note
    check_case(runner, tmp_path, 3, m.p, [("d", 0, 200000, 64)])
    assert decode((tmp_path / "o3.f1p").read_bytes()).channels[0].notes == [Note(0, 60, 10, 90), Note(300000, 62, 10, 90)]


def full_piece():
    notes = [Note(i, 60 + i % 12, 10, 90) for i in range((32768 - 48 - 16 - 4) // 6)]
    return Piece("full", 1200, 384 * 999, [Channel("lead", notes)])


def test_10_4_refused_edits_change_nothing(runner, tmp_path):
    p = eight()
    ops = [("i", 8, 0, 60, 10, 90), ("i", 0, 384 * 8, 60, 10, 90), ("i", 0, 0, 128, 10, 90), ("i", 0, 0, 1, 0, 90),
           ("i", 0, 0, 1, 65536, 90), ("i", 0, 0, 1, 10, 0), ("i", 0, 0, 1, 10, 128), ("i", 0, 0, 50, 10, 90),
           ("d", 0, 1, 50), ("d", 0, 0, 255), ("d", 9, 0, 50), ("s", 0, 0, 51, 10, 90), ("s", 0, 0, 50, 0, 90), ("u",)]
    assert runner([f"load {_save(tmp_path, 'h', p)}", *map(line, ops)]) == [ARG] * 7 + [EXISTS, NONE, NONE, ARG, NONE, ARG, NONE]
    f = full_piece()
    assert len(encode(f)) == 32768
    assert runner([f"load {_save(tmp_path, 'f', f)}", "i 0 5450 61 10 90", "i 0 383000 61 10 90", "d 0 5 65", "i 0 383000 61 10 90", "i 0 5450 61 10 90",
                   "i 0 5451 61 10 90"]) == [FULL, FULL, OK, FULL, OK, FULL]     # (far off: 5 rests more)


def _save(tmp_path, name, p):
    f = tmp_path / f"{name}.f1p"
    f.write_bytes(encode(p))
    return f


def test_10_5_undo_takes_back_64_exactly(runner, tmp_path):
    p = eight()
    src = _save(tmp_path, "u", p)
    ops = [f"i {i % 8} {i * 7} {20 + i} 30 99" for i in range(70)] + [f"s 0 {i * 96} 50 7 7" for i in range(3)]
    got = runner([f"load {src}", *ops, f"save {tmp_path / 'mid.f1p'}", *["u"] * 64, f"save {tmp_path / 'back.f1p'}", "u"])
    assert got == [OK] * 73 + [OK] * 64 + [NONE]
    m = Model(decode(encode(p)))
    for o in ops[:9]:
        k, *a = o.split()
        m.apply((k, *map(int, a)))
    assert (tmp_path / "back.f1p").read_bytes() == encode(m.p)       # the first 9 stay: only 64 are kept


def test_10_4_a_note_of_the_next_channel_is_not_this_ones(runner, tmp_path):
    p = Piece("two", 1200, 384, [Channel("lead", [Note(0, 60, 10, 90)]), Channel("bass", [Note(5, 70, 10, 90)])])
    src = _save(tmp_path, "two", p)
    assert runner([f"load {src}", "d 0 5 70", "s 0 5 70 3 3", "i 0 5 70 10 90", "d 1 5 70"]) == [NONE, NONE, OK, OK]

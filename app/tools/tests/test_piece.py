"""US-2: the .f1p format: Python writer/reader and the firmware's C reader agree."""
import random, subprocess, sys
from pathlib import Path
import pytest
from hypothesis import given, settings, strategies as st
from conftest import APP

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import (Channel, Note, Piece, PieceError, ROLES, clean_name, decode, dump, encode,  # noqa: E402
                        MAX_BYTES, BAD_CRC, BAD_HEADER, BAD_MAGIC, BAD_NCH, BAD_VERSION, BAD_EVENT, SHORT, TOO_BIG)

DUMP = APP / "build/unit/piece_dump"


@pytest.fixture(scope="module")
def cdump():
    DUMP.parent.mkdir(parents=True, exist_ok=True)
    san = subprocess.run(["sh", str(APP / "tests/sanflags.sh")], capture_output=True, text=True, check=True).stdout.split()
    r = subprocess.run(["cc", "-O1", "-g", "-Wall", "-Werror", *san,
                        "-I", str(APP / "firmware/src"), "-o", str(DUMP), str(APP / "tests/piece_dump.c")],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr

    def run(*paths):
        r = subprocess.run([str(DUMP), *map(str, paths)], capture_output=True, text=True)
        assert r.returncode == 0 and "Sanitizer" not in r.stderr, r.stderr
        return r.stdout.split("--\n")
    return run


def rand_piece(rng, nch=None, nnotes=None):
    nch = nch or rng.randint(1, 8)
    length = rng.randint(1, 96 * 4 * 200)
    chans = []
    for i in range(nch):
        k = nnotes if nnotes is not None else rng.randint(0, 120)
        notes = [Note(rng.randrange(length), rng.randint(0, 127), rng.randint(1, 65535), rng.randint(1, 127)) for _ in range(k)]
        chans.append(Channel(rng.choice(ROLES), notes, rng.randint(0, 255), rng.randint(0, 127), rng.randint(0, 127), rng.randint(0, 15)))
    name = clean_name("".join(rng.choice("abc XYZ-09é漢🎵") for _ in range(rng.randint(0, 30))))
    return Piece(name, rng.randint(300, 3000), length, chans)


def test_2_1_round_trip_8_channels_1000_notes():
    rng = random.Random(1)
    p = rand_piece(rng, nch=8, nnotes=125)
    b = encode(p)
    q = decode(b)
    assert dump(q) == dump(p)
    assert sum(len(c.notes) for c in q.channels) == 1000
    assert encode(q) == b


def test_2_2_c_reader_matches_python_on_1000_pieces(tmp_path, cdump):
    rng = random.Random(22)
    paths, pieces = [], []
    for i in range(1000):
        p = rand_piece(rng)
        f = tmp_path / f"{i}.f1p"
        f.write_bytes(encode(p))
        paths.append(f); pieces.append(p)
    outs = []
    for k in range(0, 1000, 250):                    # (argv length)
        outs += cdump(*paths[k:k + 250])
    assert len(outs) == 1000
    for p, o in zip(pieces, outs):
        assert o == dump(p)


@settings(max_examples=300, deadline=None)
@given(st.integers(0, 2**32 - 1))
def test_2_2_python_round_trip_property(seed):
    p = rand_piece(random.Random(seed))
    assert dump(decode(encode(p))) == dump(p)


def corrupt_cases():
    base = bytearray(encode(rand_piece(random.Random(5), nch=3, nnotes=10)))

    def recrc(b):
        import zlib, struct
        b = bytearray(b)
        b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF)
        return bytes(b)

    def with_byte(off, val):
        b = bytearray(base); b[off] = val; return recrc(b)
    flipped = bytearray(base); flipped[60] ^= 0xFF
    big = bytearray(base) + bytes(MAX_BYTES)
    return {
        "truncated": (bytes(base[:-7]), SHORT),
        "crc flipped": (bytes(flipped), BAD_CRC),
        "wrong magic": (with_byte(0, ord("X")), BAD_MAGIC),
        "version 2": (with_byte(4, 2), BAD_VERSION),
        "nch 9": (with_byte(5, 9), BAD_NCH),
        "nch 0": (with_byte(5, 0), BAD_NCH),
        "empty name": (with_byte(16, 0), BAD_HEADER),
        "control char in name": (with_byte(17, 7), BAD_HEADER),
        "event past length": (recrc(bytes(base[:12]) + (1).to_bytes(4, "little") + bytes(base[16:])), BAD_EVENT),
        "33 KB": (bytes(big[:33 * 1024]), TOO_BIG),
        "empty file": (b"", SHORT),
        "header only": (bytes(base[:48]), SHORT),
    }


@pytest.mark.parametrize("name", list(corrupt_cases()))
def test_2_3_hostile_pieces_rejected_with_codes(name, tmp_path, cdump):
    data, code = corrupt_cases()[name]
    f = tmp_path / "x.f1p"
    f.write_bytes(data)
    out = cdump(f)[0]
    assert out == f"ERR {code}\n", (name, out)
    with pytest.raises(PieceError) as e:
        decode(data)
    assert e.value.code == code


def test_2_3_c_reader_never_overreads_random_garbage(tmp_path, cdump):
    rng = random.Random(23)
    good = encode(rand_piece(rng, nch=4, nnotes=20))
    paths = []
    for i in range(300):
        b = bytearray(good)
        for _ in range(rng.randint(1, 8)):
            b[rng.randrange(len(b))] = rng.randrange(256)
        if i % 3 == 0:
            b = b[: rng.randrange(len(b))]
        f = tmp_path / f"g{i}.f1p"; f.write_bytes(bytes(b)); paths.append(f)
    outs = cdump(*paths)
    assert len(outs) == 300                      # ran to the end under ASan/UBSan: no overread, no UB


@pytest.mark.parametrize("raw,want", [
    ("Electronic House 1", "Electronic House 1"),
    ("Café del Mar", "Cafe del Mar"),
    ("漢字🎵", "untitled"),
    ("שלום abc", "abc"),
    ("a\nb\0c\td", "a b c d"),
    ("", "untitled"),
    ("   ", "untitled"),
    ("x" * 40, "x" * 24),
    ("'; rm -rf / $(x)", "'; rm -rf / $(x)"),
])
def test_2_4_names_are_printable_ascii(raw, want):
    got = clean_name(raw)
    assert got == want
    assert 1 <= len(got) <= 24 and all(32 <= ord(c) <= 126 for c in got)


def test_2_4_encode_refuses_raw_non_ascii_name():
    p = rand_piece(random.Random(3), nch=1, nnotes=1)
    p.name = "Café"
    with pytest.raises(PieceError) as e:
        encode(p)
    assert e.value.code == BAD_HEADER


def test_2_1_piece_of_silent_channels_is_valid(tmp_path, cdump):
    p = Piece("Silence", 1200, 384, [Channel("pad"), Channel("drums")])
    b = encode(p)
    assert len(b) == 48 + 32 + 4
    assert dump(decode(b)) == dump(p)
    f = tmp_path / "s.f1p"; f.write_bytes(b)
    assert cdump(f)[0] == dump(p)


def test_2_3_errors_name_the_channel():
    p = Piece("x", 1200, 100, [Channel("pad", [Note(1, 60, 1, 1)]), Channel("bass", [Note(99, 40, 1, 1)])])
    b = bytearray(encode(p))
    import struct, zlib
    off = 48 + 32 + 6                              # channel 2's first event: delta 99 -> 120 (past the end)
    b[off:off + 2] = struct.pack("<H", 120)
    b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF)
    with pytest.raises(PieceError) as e:
        decode(bytes(b))
    assert e.value.code == BAD_EVENT and "channel 2" in str(e.value)

"""US-5: render_piece plays a piece through the firmware's own player + synth into a WAV (hear it without the emulator)."""
import subprocess, wave
from conftest import APP
from fm1c.piece import Piece, Channel, Note, encode


def test_5_1_render_piece_plays_a_piece(tmp_path):
    exe = tmp_path / "render_piece"
    subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                    str(APP / "tests/render_piece.c"), "-lm"], check=True)
    notes = [Note(t * 96, 60 + t % 5, 80, 100) for t in range(8)]
    kicks = [Note(t * 96, 36, 20, 120) for t in range(8)]
    src = tmp_path / "p.f1p"
    src.write_bytes(encode(Piece("render", 1200, 768, [Channel("lead", notes), Channel("drums", kicks)])))
    out = tmp_path / "p.wav"
    r = subprocess.run([str(exe), str(src), str(out), "2"], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    assert "clipped 0.0000%" in r.stdout
    with wave.open(str(out)) as w:
        assert (w.getframerate(), w.getnchannels(), w.getsampwidth()) == (44100, 2, 2)
        data = w.readframes(w.getnframes())
    assert max(abs(int.from_bytes(data[i:i + 2], "little", signed=True)) for i in range(0, len(data), 2)) > 1000
    bad = tmp_path / "bad.f1p"
    bad.write_bytes(b"junk")
    r = subprocess.run([str(exe), str(bad), str(out)], capture_output=True, text=True)
    assert r.returncode == 2 and "not a valid piece" in r.stderr

import { describe, expect, it } from "vitest";
import { clearNew, markNew, openNow, pieceLabel } from "./pieces";

describe("pieces", () => {
  it("58.3 shows saved file names as readable titles", () => {
    expect(pieceLabel("slow-sad-lofi-in-d-minor-482910.t2m.json")).toBe("Slow sad lofi in d minor");
    expect(pieceLabel("pop-starter-1.t2m.json")).toBe("Pop starter");
    expect(pieceLabel("my_song.t2m.json")).toBe("My song");
    expect(pieceLabel("piece-7.t2m.json")).toBe("Piece");
    expect(pieceLabel("12345.t2m.json")).toBe("12345");
    expect(pieceLabel("-.t2m.json")).toBe("-.t2m.json");
  });

  it("58.4 a new piece opens at once when nothing plays, and waits in the list while music plays", () => {
    expect(openNow(false)).toBe(true);
    expect(openNow(true)).toBe(false);
  });

  it("58.5 / 59.7 new marks are added once, newest first (a repeat moves to the front), and removed when the piece is opened", () => {
    let marks: string[] = [];
    marks = markNew(marks, "a.t2m.json");
    marks = markNew(marks, "b.t2m.json");
    expect(marks).toEqual(["b.t2m.json", "a.t2m.json"]);
    marks = markNew(marks, "a.t2m.json");
    expect(marks).toEqual(["a.t2m.json", "b.t2m.json"]);
    expect(clearNew(marks, "a.t2m.json")).toEqual(["b.t2m.json"]);
    expect(clearNew(marks, "zzz")).toEqual(marks);
  });
});

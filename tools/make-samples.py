#!/usr/bin/env python3
"""Write the sample dictionaries in samples/ (made up for Kosha, free to use).

  samples/en-ru/Kosha Sample En-Ru.dsl           UTF-16 LE with BOM, like Lingvo writes them
  samples/en-ru/Kosha Sample En-Ru_abrv.dsl      its abbreviations ([p] labels)
  samples/en-ru/Kosha Sample En-Ru.dsl.files.zip a sound (apple.wav) and a picture (cat.png)
  samples/ru-en/Kosha Sample Ru-En.dsl.dz        UTF-8, compressed with dictzip

Run from the repository: python3 tools/make-samples.py
"""
import io
import math
import struct
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "samples"

EN_RU = r"""#NAME "Kosha Sample En-Ru"
#INDEX_LANGUAGE "English"
#CONTENTS_LANGUAGE "Russian"

{{Sample dictionary for Kosha: every DSL feature the reader handles, in a few articles.}}
apple
	[m0][b]apple[/b] [t]ˈæp(ə)l[/t] [s]apple.wav[/s]
	[m1][p]n[/p]
	[m2][trn]яблоко[/trn]
	[m3][ex][lang id=1033]an ~ a day keeps the doctor away[/lang] — [lang id=1049]кто ест яблоко в день, тот не болеет[/lang][/ex]
	[m2][*][com]в разных сортах:[/com] [trn]антоновка, ранет[/trn][/*]
	[m1]see also <<pear>>, [ref]apple tree[/ref]
	@ apple tree
	[m2][p]n[/p] [trn]яблоня[/trn]
	@
pear
	[m0][b]pear[/b] [t]peə[/t]
	[m1][p]n[/p] [trn]груша[/trn]
bring (in)
	[m0][b]bring[/b] [t]brɪŋ[/t] [c gray](brought, brought)[/c]
	[m1][p]v[/p]
	[m2][b]1.[/b] [trn]приносить, приводить[/trn]
	[m3][ex]~ me the book — принеси мне книгу[/ex]
	[m2][b]2.[/b] [b]bring in[/b] [trn]вносить; вводить ([i]закон[/i])[/trn]
go{es}
	[m0][b]go[/b] [t]gəʊ[/t] [c gray](went, gone)[/c]
	[m1][p]v[/p] [trn]идти, ехать; уходить[/trn]
	[m2][ex]it's time to go — пора идти[/ex]
look (up)
	[m1][p]v[/p] [trn]смотреть[/trn]
	[m2][b]look up[/b] [trn]искать ([i]в словаре[/i])[/trn]
	[m3][ex]~ a word in Kosha — посмотреть слово в Kosha[/ex]
colour
color
	[m1][p]n[/p] [trn]цвет[/trn]; [p]v[/p] [trn]красить[/trn]
	[m2][com]color — американское написание[/com]
cat
	[m0][b]cat[/b] [t]kæt[/t]
	[m1][p]n[/p] [trn]кошка, кот[/trn]
	[m1][s]cat.png[/s]
café
	[m1][p]n[/p] [trn]кафе[/trn]
rock’n’roll
	[m1][p]n[/p] [trn]рок-н-ролл[/trn]
house
	[m0][b]house[/b] [t]haʊs[/t]
	[m1][p]n[/p] [b]1.[/b] [trn]дом[/trn]
	[m1][b]2.[/b] [p]разг[/p] [trn]хаус ([i]музыка[/i])[/trn]
	[m1][url]https://en.wiktionary.org/wiki/house[/url]
run
	[m1][p]v[/p] [trn]бежать[/trn]
	[m2][b]run [i]away[/i][/b] — [trn]убегать[/trn]
	[m2]{{misnested tags on purpose:}}[b][i]run out[/b] of[/i] — [trn]кончаться[/trn]
"""

EN_RU_ABRV = r"""#NAME "Kosha Sample En-Ru Abbreviations"
#INDEX_LANGUAGE "English"
#CONTENTS_LANGUAGE "Russian"

n
	имя существительное
v
	глагол
adj
	имя прилагательное
разг
	разговорное
"""

RU_EN = r"""#NAME "Kosha Sample Ru-En"
#INDEX_LANGUAGE "Russian"
#CONTENTS_LANGUAGE "English"

яблоко
	[m1][trn]apple[/trn]
	[m2][ex]яблоко от яблони недалеко падает — like father, like son[/ex]
ёлка
	[m1][trn]fir tree; Christmas tree[/trn]
дом
	[m1][trn]house, home[/trn]
	[m2][ex]быть как дома — make oneself at home[/ex]
бежать
	[m1][trn]run[/trn], see <<бег>>
бег
	[m1][trn]running, race[/trn]
кошка
	[m1][trn]cat[/trn]
"""


def wav(seconds=0.4, freq=660, rate=16000):
    n = int(seconds * rate)
    frames = b"".join(
        struct.pack("<h", int(12000 * math.sin(2 * math.pi * freq * i / rate) * min(1, (n - i) / 800, i / 200)))
        for i in range(n)
    )
    head = b"RIFF" + struct.pack("<I", 36 + len(frames)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16)
    return head + b"data" + struct.pack("<I", len(frames)) + frames


def png(w=96, h=64):
    # a simple cat face: dark circle with two ears and eyes, on transparent ground
    rows = []
    for y in range(h):
        row = bytearray([0])
        for x in range(w):
            cx, cy, r = w / 2, h / 2 + 6, 24
            inside = (x - cx) ** 2 + (y - cy) ** 2 <= r * r
            ear = y < cy and abs(x - (cx - 14)) + (y - 8) * 0.9 < 10 or y < cy and abs(x - (cx + 14)) + (y - 8) * 0.9 < 10
            eye = (x - (cx - 9)) ** 2 + (y - cy + 2) ** 2 <= 9 or (x - (cx + 9)) ** 2 + (y - cy + 2) ** 2 <= 9
            if eye and inside:
                row += bytes([250, 210, 80, 255])
            elif inside or ear:
                row += bytes([70, 70, 80, 255])
            else:
                row += bytes([0, 0, 0, 0])
        rows.append(bytes(row))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(
        b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")


def dictzip(data: bytes, chunk_len=58315) -> bytes:
    """gzip with the RA field: every chunk flushed so it can be inflated on its own."""
    comp = zlib.compressobj(9, zlib.DEFLATED, -15)
    bodies = []
    for i in range(0, max(len(data), 1), chunk_len):
        bodies.append(comp.compress(data[i:i + chunk_len]) + comp.flush(zlib.Z_FULL_FLUSH))
    bodies[-1] += comp.flush(zlib.Z_FINISH)
    ra = struct.pack("<HHH", 1, chunk_len, len(bodies)) + b"".join(struct.pack("<H", len(b)) for b in bodies)
    extra = b"RA" + struct.pack("<H", len(ra)) + ra
    head = b"\x1f\x8b\x08\x04" + b"\0\0\0\0" + b"\x02\x03" + struct.pack("<H", len(extra)) + extra
    return head + b"".join(bodies) + struct.pack("<II", zlib.crc32(data) & 0xFFFFFFFF, len(data) & 0xFFFFFFFF)


def utf16(text: str) -> bytes:
    return b"\xff\xfe" + text.replace("\n", "\r\n").encode("utf-16-le")


def main():
    en = ROOT / "en-ru"
    ru = ROOT / "ru-en"
    en.mkdir(parents=True, exist_ok=True)
    ru.mkdir(parents=True, exist_ok=True)
    (en / "Kosha Sample En-Ru.dsl").write_bytes(utf16(EN_RU))
    (en / "Kosha Sample En-Ru_abrv.dsl").write_bytes(utf16(EN_RU_ABRV))
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in (("apple.wav", wav()), ("cat.png", png())):
            info = zipfile.ZipInfo(name, date_time=(2026, 10, 23, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, data)
    (en / "Kosha Sample En-Ru.dsl.files.zip").write_bytes(buf.getvalue())
    (ru / "Kosha Sample Ru-En.dsl.dz").write_bytes(dictzip(b"\xef\xbb\xbf" + RU_EN.encode("utf-8")))
    for p in sorted(ROOT.rglob("*")):
        if p.is_file():
            print(f"{p.relative_to(ROOT.parent)}  {p.stat().st_size} bytes")


if __name__ == "__main__":
    main()

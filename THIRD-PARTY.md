# Third-party material in the launcher page

The page carries two pieces of other people's work. Nothing of Minecraft's own font or textures is used.

## Monocraft (the headings' font)

- Source: https://github.com/IdreesInc/Monocraft, `dist/Monocraft-ttf/Monocraft.ttf` (release v4.2.1).
- Licence: SIL Open Font License 1.1, https://github.com/IdreesInc/Monocraft/blob/main/LICENSE. Copyright (c) 2022, Idrees Hassan.
- What is used: a subset holding only printable ASCII and a few punctuation marks (`· × – — ’ … →`), without ligatures, as WOFF, embedded in `index.html` as a data URL (about 3 KB). The OFL allows a subset to be embedded in a document; it is not sold on its own.
- How it was made: `pyftsubset Monocraft.ttf --unicodes="U+0020-007E,U+00B7,U+00D7,U+2013,U+2014,U+2019,U+2026,U+2192" --layout-features="" --flavor=woff`.

## Pixelarticons (the icons)

- Source: https://github.com/halfmage/pixelarticons, the SVGs `home`, `map`, `play`, `sliders`, `arrow-left`.
- Licence: MIT, https://github.com/halfmage/pixelarticons/blob/master/LICENSE.

```
MIT License

Copyright (c) 2019 Gerrit Halfmann

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The stop square is a plain square drawn on the same 24-pixel grid.

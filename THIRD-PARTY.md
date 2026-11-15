# Third-party material in the launcher page

The page carries two pieces of other people's work. Nothing of Minecraft's own font or textures is used.

## IBM Plex Mono (the page's font)

- Source: https://github.com/IBM/plex, release `@ibm/plex-mono@2.5.0`, file `ibm-plex-mono.zip`.
- Licence: SIL Open Font License 1.1, https://github.com/IBM/plex/blob/master/LICENSE.txt. Copyright © 2017 IBM Corp. with Reserved Font Name "Plex".
- What is used: IBM's own split cuts, unchanged, from `fonts/split/woff2`: `IBMPlexMono-Regular-Latin1.woff2`, `IBMPlexMono-SemiBold-Latin1.woff2` and `IBMPlexMono-Regular-Pi.woff2` (arrows and signs), embedded in `index.html` as data URLs (about 50 KB) with the unicode ranges of IBM's own stylesheet. The OFL allows the font to be embedded in a document; it is not sold on its own. The world preview page embeds the same three files.

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

The native launcher (`native`) draws the same Pixelarticons paths, with the same licence and notice, through rust-native-ui.

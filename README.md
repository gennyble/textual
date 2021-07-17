# Textual
Text rendering as a service!

Ever want to say something, but bigger? Want to post an image of text, so you
can style it in fun ways, but want to remain accessible? Textual can help you!

The URL below will provide the image, also shown below.  
https://textual.bookcase.name/?text=Textual&font=Righteous&color=155

<center>

![Text, in the font Righteous, reading "Textual"](https://textual.bookcase.name/?text=Textual&font=Righteous&color=155&forceraw)

</center>

## How's it work?
Textual is something I made when I wanted to play with text rendering. It uses
a layout library I wrote, [fontster][fontster], to get the positions of the
glyphs. It takes these glyph positions and draws them to an image using the
necessary styling.

The font specified is first looked for in a cache of fonts locally, but if none
is found it is pulled from Google Fonts.

Currently textual is one big binary that serves images over HTTP, but I'd like
to eventually break it out into separate rendering and web-service crates.

[fontster]: https://github.com/gennyble/fontster
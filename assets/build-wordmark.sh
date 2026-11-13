#!/bin/sh
# Export the wordmark as light and dark SVGs and make the cursor blink.
# Needs Typst and the Geist Mono font (nixpkgs#geist-font).
# Usage: sh build-wordmark.sh            (fonts from TYPST_FONT_PATHS or the system)
#        FONTS=/path/to/fonts sh build-wordmark.sh
set -eu
cd "$(dirname "$0")"

# The blink lives inside the SVG, so it also runs when GitHub shows the file as an image.
# Viewers who ask their system for reduced motion get a steady cursor.
style='<style>.cursor{animation:blink 1.2s steps(1,end) infinite}@keyframes blink{50%{opacity:0}}@media (prefers-reduced-motion:reduce){.cursor{animation:none}}</style>'

build() { # <theme> <ink>
  out="wordmark-$1.svg"
  typst compile ${FONTS:+--font-path "$FONTS"} \
    --input ink="$2" wordmark.typ "$out"
  # The cursor is the only element in Nushell green: tag it, then add the style block.
  sed -i -e 's|fill="#4e9a06"|fill="#4e9a06" class="cursor"|' \
         -e "s|\(<svg[^>]*>\)|\1$style|" "$out"
  echo "$out"
}

build light "#24292f"
build dark "#e6edf3"

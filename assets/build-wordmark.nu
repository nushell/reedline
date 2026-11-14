#!/usr/bin/env nu
# Export the wordmark as light and dark SVGs and make the cursor blink.
# Needs Typst and the Geist Mono font (nixpkgs#geist-font).
#
#   nu build-wordmark.nu                  fonts from TYPST_FONT_PATHS or the system
#   nu build-wordmark.nu --fonts <dir>    fonts from a folder

# The blink lives inside the SVG, so it also runs when GitHub shows the file as an image.
# Viewers who ask their system for reduced motion get a steady cursor.
const style = '<style>.cursor{animation:blink 1.2s steps(1,end) infinite}@keyframes blink{50%{opacity:0}}@media (prefers-reduced-motion:reduce){.cursor{animation:none}}</style>'

# The cursor is the only element in this colour, which is how it is found below.
const cursor_fill = 'fill="#4e9a06"'

def build [theme: string, ink: string, fonts?: path] {
    let source = $env.FILE_PWD | path join wordmark.typ
    let out = $env.FILE_PWD | path join $"wordmark-($theme).svg"
    let font_args = if $fonts == null { [] } else { [--font-path $fonts] }

    ^typst compile ...$font_args --input $"ink=($ink)" $source $out

    # Tag the cursor, then put the style block right after the opening <svg> tag.
    let svg = open --raw $out
        | str replace $cursor_fill $'($cursor_fill) class="cursor"'
        | str replace --regex '(<svg[^>]*>)' ('${1}' + $style)
    # A Typst release that writes the fill or the root tag differently would
    # otherwise give a wordmark that silently no longer blinks.
    let tagged = $svg | str contains 'class="cursor"'
    let styled = $svg | str contains '<style>'
    if not ($tagged and $styled) {
        error make {msg: $"($out): cursor or root tag not found, the blink was not added"}
    }
    $svg | save --force $out
    print $out
}

def main [
    --fonts: path # folder to load fonts from, in addition to the system fonts
] {
    build light "#24292f" $fonts
    build dark "#e6edf3" $fonts
}

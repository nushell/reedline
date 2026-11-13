// The reedline wordmark (F5). Build with build-wordmark.sh, which exports
// light and dark SVGs and adds the cursor blink.
//   --input font="Geist Mono"   typeface
//   --input ink="#24292f"       colour of everything except the cursor

#let ink = rgb(sys.inputs.at("ink", default: "#24292f"))
#let font = sys.inputs.at("font", default: "Geist Mono")
#let cursor = rgb("#4e9a06") // Nushell green; the build script finds the cursor by this colour
#let size = 72pt

#set page(width: auto, height: auto, margin: (x: 10pt, top: 10pt, bottom: 24pt), fill: none)
#set text(font: font, weight: "bold", size: size, fill: ink, fallback: false,
  top-edge: "ascender", bottom-edge: "descender")
#set par(leading: 0pt, spacing: 0pt)

#let sp = sym.space.nobreak

#sp#sp#sp;╲╻╱ \
#underline(stroke: size / 20 + ink, offset: size / 5, evade: false, background: true)[reed│ine#text(fill: cursor)[▏]]

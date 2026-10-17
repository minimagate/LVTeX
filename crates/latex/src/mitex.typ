// Scope for MiTeX 0.2.4 output, evaluated in math mode by `convert`.
// The vendored MiTeX command library (see crates/latex/vendor/mitex/README.md)
// supplies the base scope. The entries below override it with the local
// definitions that already render the project's formulas, so those stay unchanged,
// plus fixes for names that Typst 0.15 no longer provides.
#let mitex-scope = package.scope + (
  // `std.sym`, because the vendored prelude shadows `sym` with its own helper.
  "diff": std.sym.partial,
  "angle": symbol("∠", ("l", "⟨"), ("r", "⟩"), ("arc", "∡"), ("spheric", "∢")),
  "matrix": math.mat.with(delim: none),
  "pmatrix": math.mat.with(delim: "("),
  "bmatrix": math.mat.with(delim: "["),
  "Bmatrix": math.mat.with(delim: "{"),
  "vmatrix": math.mat.with(delim: "|"),
  "Vmatrix": math.mat.with(delim: ("‖", "‖")),
  "aligned": body => math.display(body),
  "textmath": body => body,
  "cases": (..branches) => {
    let spaced(row) = if row.has("children") {
      row.children.map(c => if c == $&$.body { c + h(1em) } else { c }).join()
    } else { row }
    math.cases(..branches.pos().map(spaced))
  },
  "mitexsqrt": (..args) => {
    let values = args.pos()
    if values.len() == 1 {
      math.sqrt(values.first())
    } else if values.len() == 2 {
      // MiTeX includes literal square brackets around the optional root index.
      let index = values.first().children.filter(c => c != [\[] and c != [\]]).join()
      math.root(index, values.last())
    } else {
      panic("sqrt expects a radicand and an optional index")
    }
  },
  // Typst 0.15 has no `angle.l`, so the bra-ket helpers use `chevron`.
  "bra": it => $chevron.l it|$,
  "ket": it => $lr(|it chevron.r)$,
  "braket": it => $chevron.l it chevron.r$,
  "Bra": it => $lr(chevron.l it|)$,
  "Ket": it => $lr(|it chevron.r)$,
  "Braket": it => $lr(chevron.l it chevron.r)$,
  // Emitted names that Typst 0.15 no longer defines under the converter's spelling.
  "sect": std.sym.inter,
  "join": std.sym.bowtie.big,
  "argmax": math.op("arg max", limits: true),
  "argmin": math.op("arg min", limits: true),
  "ohm": (inv: math.upright("℧")),
)

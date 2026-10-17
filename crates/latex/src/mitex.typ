// Compatibility definitions for MiTeX 0.2.4 output and Typst 0.15.1.
// This is a deliberately small, explicit bridge, not the full MiTeX package.
// Unknown helpers fail compilation rather than silently changing the formula.
#let diff = sym.partial
#let angle = symbol("∠", ("l", "⟨"), ("r", "⟩"))
#let matrix = math.mat.with(delim: none)
#let pmatrix = math.mat.with(delim: "(")
#let bmatrix = math.mat.with(delim: "[")
#let Bmatrix = math.mat.with(delim: "{")
#let vmatrix = math.mat.with(delim: "|")
#let Vmatrix = math.mat.with(delim: "||")
#let aligned(body) = math.display(body)
#let textmath(body) = body
#let cases(..branches) = {
  let spaced(row) = if row.has("children") {
    row.children.map(c => if c == $&$.body { c + h(1em) } else { c }).join()
  } else { row }
  math.cases(..branches.pos().map(spaced))
}
#let mitexsqrt(..args) = {
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
}

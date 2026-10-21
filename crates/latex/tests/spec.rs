use lvtex_latex::{PRELUDE, convert};
use lvtex_render::math::{Renderer, SCALE};
use mitex::{ArgPattern, ArgShape, CommandSpecItem};

#[path = "fixtures/spec_allowlist.rs"]
mod allowlist;
use allowlist::SPEC_ALLOWLIST;

/// Builds a minimal LaTeX sample for a spec item from its argument shape.
fn spec_sample(name: &str, item: &CommandSpecItem) -> String {
    let terms = |n: usize| "{x}".repeat(n);
    match item {
        CommandSpecItem::Cmd(cmd) => match &cmd.args {
            ArgShape::Right { pattern } => match pattern {
                ArgPattern::None => format!(r"\{name}"),
                ArgPattern::FixedLenTerm { len } => format!(r"\{name}{}", terms(*len as usize)),
                ArgPattern::RangeLenTerm { min, .. } => {
                    format!(r"\{name}{}", terms(*min as usize))
                }
                ArgPattern::Greedy => format!(r"\{name} x"),
                ArgPattern::Glob { .. } => format!(r"\{name}{{x}}"),
            },
            ArgShape::Left1 => format!(r"x \{name}"),
            ArgShape::InfixGreedy => format!(r"a \{name} b"),
        },
        CommandSpecItem::Env(env) => {
            let count = match &env.args {
                ArgPattern::FixedLenTerm { len } => *len as usize,
                ArgPattern::RangeLenTerm { min, .. } => *min as usize,
                _ => 0,
            };
            format!(r"\begin{{{name}}}{}x\end{{{name}}}", "{c}".repeat(count))
        }
    }
}

/// Spacing and phantom commands, which draw no ink on their own. They are
/// also tried between text. Commands that only matter in preambles or
/// labels are not, so they stay allowlisted.
const IN_CONTEXT: &[&str] = &[
    "",
    "!",
    "*",
    ",",
    ":",
    ";",
    ">",
    "Box",
    "enspace",
    "hphantom",
    "hskip",
    "kern",
    "mathstrut",
    "medspace",
    "mkern",
    "mskip",
    "negmedspace",
    "negthickspace",
    "negthinmedspace",
    "negthinspace",
    "nobreak",
    "nobreakspace",
    "phantom",
    "qquad",
    "quad",
    "space",
    "thickspace",
    "thinspace",
    "vphantom",
];

/// Samples tried for a spec item, in order. The in-context and explicit
/// samples cover the spacing commands and the color and length commands
/// that the generic sample cannot supply.
fn spec_samples(name: &str, item: &CommandSpecItem) -> Vec<String> {
    let mut samples = vec![spec_sample(name, item)];
    if let CommandSpecItem::Cmd(cmd) = item
        && let ArgShape::Right { pattern } = &cmd.args
        && IN_CONTEXT.contains(&name)
    {
        // Only fixed-length shapes: glob and greedy samples would
        // swallow the trailing text, and the converter panics on some.
        let dims = match pattern {
            ArgPattern::None => Some(0),
            ArgPattern::FixedLenTerm { len } => Some(*len as usize),
            ArgPattern::RangeLenTerm { min, .. } => Some(*min as usize),
            ArgPattern::Greedy | ArgPattern::Glob { .. } => None,
        };
        if let Some(n) = dims {
            samples.push(if n == 0 {
                format!(r"x \{name} y")
            } else {
                format!(r"x\{name}{}y", "{1em}".repeat(n))
            });
        }
    }
    if let Some(explicit) = explicit_sample(name) {
        samples.push(explicit.to_string());
    }
    samples
}

/// Hand-written samples for commands whose arguments must be a color or length.
fn explicit_sample(name: &str) -> Option<&'static str> {
    Some(match name {
        "color" => r"\color{red}x",
        "textcolor" => r"\textcolor{red}{x}",
        "colorbox" => r"\colorbox{red}{x}",
        "hspace" => r"x\hspace{1em}y",
        "vspace" => r"x\vspace{1em}y",
        "raisebox" => r"x\raisebox{1pt}{x}y",
        _ => return None,
    })
}

#[test]
fn every_spec_command_renders_or_is_allowlisted() {
    let mut renderer = Renderer::with_prelude(PRELUDE);
    let mut failures = Vec::new();
    let mut total = 0;
    for (name, item) in mitex_spec_gen::DEFAULT_SPEC.items() {
        total += 1;
        // An item passes when any of its samples renders.
        let mut last = None;
        let mut passed = false;
        for sample in spec_samples(name, item) {
            let result = convert(&sample)
                .and_then(|body| renderer.render_math(&body, true, SCALE).map(|_| ()));
            match result {
                Ok(()) => {
                    passed = true;
                    break;
                }
                Err(error) => last = Some((sample, error)),
            }
        }
        if !passed {
            let (sample, error) = last.expect("at least one sample");
            failures.push((name.to_string(), sample, error));
        }
    }
    let failing: Vec<&str> = failures.iter().map(|(n, _, _)| n.as_str()).collect();
    let allowed: Vec<&str> = SPEC_ALLOWLIST.iter().map(|(n, _)| *n).collect();
    let mut unexpected: Vec<_> = failures
        .iter()
        .filter(|(n, _, _)| !allowed.contains(&n.as_str()))
        .collect();
    unexpected.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(
        unexpected.is_empty(),
        "{} of {total} spec items fail without allowlist entries:\n{}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|(n, s, e)| format!("  {n}: {s} => {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut stale: Vec<&str> = allowed
        .iter()
        .copied()
        .filter(|n| !failing.contains(n))
        .collect();
    stale.sort_unstable();
    assert!(stale.is_empty(), "allowlisted but now passing: {stale:?}");
}

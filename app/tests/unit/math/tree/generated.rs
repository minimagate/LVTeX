use super::*;

struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % n
    }
}

#[test]
fn random_edits_never_panic_and_always_round_trip() {
    const SYMBOLS: [(&str, &str); 4] = [("x", "x"), ("1", "1"), ("+", "+"), (r"\alpha", "a")];
    const PAIRS: [(&str, &str); 6] = [
        ("(", ")"),
        ("[", "]"),
        ("{", "}"),
        (".", "."),
        ("|", "|"),
        (r"\langle", r"\rangle"),
    ];
    let caret = format!(r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}");
    for seed in 1..=30 {
        let mut rng = Lcg(seed);
        let mut t = MathTree::new();
        for step in 0..300 {
            match rng.below(22) {
                0..=3 => {
                    let (latex, keys) = SYMBOLS[rng.below(SYMBOLS.len())];
                    t.insert_symbol(latex, keys);
                }
                4 => t.make_script(rng.below(2) == 0),
                5 => t.make_prime(),
                6 => t.make_fraction(),
                7 => t.make_sqrt(),
                8 => t.make_root(),
                9 => {
                    let (left, right) = PAIRS[rng.below(PAIRS.len())];
                    t.make_delimited(left, right);
                }
                10 => t.left(),
                11 => t.right(),
                12 => t.up(),
                13 => t.down(),
                14 => t.next_argument(),
                15 => t.previous_argument(),
                16 => t.exit(rng.below(2) == 0),
                17 => t.backspace(),
                18 => t.delete(),
                19 => t.remove_box(),
                20 => t.unwrap_enclosing(),
                _ => {
                    let right = PAIRS[rng.below(PAIRS.len())].1;
                    t.set_right_delimiter(right);
                }
            }
            let latex = t.latex();
            assert_eq!(
                MathTree::parse(&latex).latex(),
                latex,
                "seed {seed} step {step}"
            );
            assert_eq!(
                t.latex_with_caret().matches(&caret).count(),
                1,
                "seed {seed} step {step}"
            );
        }
    }
}

use super::*;
use crate::config::CARET_SENTINEL;

fn sym(t: &mut MathTree, s: &str) {
    t.insert_symbol(s, s);
}

fn build(f: impl FnOnce(&mut MathTree)) -> MathTree {
    let mut t = MathTree::new();
    f(&mut t);
    t
}

type Position = (Vec<Step>, usize);

fn walk(t: &mut MathTree, forward: bool, steps: usize) -> Vec<Position> {
    let mut seen = vec![(t.path.clone(), t.pos)];
    for _ in 0..steps {
        if forward {
            t.right();
        } else {
            t.left();
        }
        seen.push((t.path.clone(), t.pos));
    }
    seen
}

fn braces_balance(s: &str) -> bool {
    let mut depth = 0i32;
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'\\' => {
                bytes.next();
            }
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

mod editing;
mod generated;
mod navigation;
mod serialization;

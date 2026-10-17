//! Hand-written Typst counterparts isolate layout/raster cost. They are not
//! evidence that MiTeX translates these LaTeX expressions correctly.
pub struct Formula {
    pub name: &'static str,
    pub latex: &'static str,
    pub typst: &'static str,
}

pub const FORMULAS: &[Formula] = &[
    Formula {
        name: "Gaussian integral",
        latex: r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
        typst: r#"integral_0^oo e^(-x^2) dif x = frac(sqrt(pi), 2)"#,
    },
    Formula {
        name: "Finite sum",
        latex: r"\sum_{k=1}^{n}\frac{1}{k^2}",
        typst: r#"sum_(k=1)^n frac(1,k^2)"#,
    },
    Formula {
        name: "Nested fractions",
        latex: r"\frac{1+\frac{a}{b}}{1-\frac{c}{d}}",
        typst: r#"frac(1+frac(a,b),1-frac(c,d))"#,
    },
    Formula {
        name: "Quadratic formula",
        latex: r"x=\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
        typst: r#"x=frac(-b plus.minus sqrt(b^2-4a c),2a)"#,
    },
    Formula {
        name: "Two by two matrix",
        latex: r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        typst: r#"mat(delim: "(", a,b;c,d)"#,
    },
    Formula {
        name: "Three by three matrix",
        latex: r"\begin{bmatrix}1&0&0\\0&1&0\\0&0&1\end{bmatrix}",
        typst: r#"mat(delim: "[", 1,0,0;0,1,0;0,0,1)"#,
    },
    Formula {
        name: "Aligned equations",
        latex: r"\begin{aligned}(a+b)^2&=a^2+2ab+b^2\\(a-b)^2&=a^2-2ab+b^2\end{aligned}",
        typst: r#"(a+b)^2 &= a^2+2a b+b^2 \ (a-b)^2 &= a^2-2a b+b^2"#,
    },
    Formula {
        name: "Piecewise function",
        latex: r"f(x)=\begin{cases}x^2&x\geq0\\-x&x<0\end{cases}",
        typst: r#"f(x)=cases(x^2 & quad x>=0, -x & quad x<0)"#,
    },
    Formula {
        name: "Greek alphabet",
        latex: r"\alpha+\beta+\gamma+\delta+\varepsilon+\varphi+\Omega",
        typst: r#"alpha+beta+gamma+delta+epsilon+phi+Omega"#,
    },
    Formula {
        name: "Nested scripts",
        latex: r"a_{i_j}^{n+1}+x^{y^z}+T^{\mu}_{\nu}",
        typst: r#"a_(i_j)^(n+1)+x^(y^z)+T^mu_nu"#,
    },
    Formula {
        name: "Limit",
        latex: r"\lim_{x\to0}\frac{\sin x}{x}=1",
        typst: r#"lim_(x arrow.r 0) frac(sin x,x)=1"#,
    },
    Formula {
        name: "Partial derivative",
        latex: r"\frac{\partial^2 u}{\partial t^2}=c^2\nabla^2u",
        typst: r#"frac(partial^2 u,partial t^2)=c^2 nabla^2 u"#,
    },
    Formula {
        name: "Contour integral",
        latex: r"\oint_C\frac{dz}{z}=2\pi i",
        typst: r#"integral.cont_C frac(dif z,z)=2 pi i"#,
    },
    Formula {
        name: "Set notation",
        latex: r"A=\{x\in\mathbb{R}:x^2<2\}\subseteq\mathbb{R}",
        typst: r#"A=lr(\{x in RR:x^2<2\}) subset.eq RR"#,
    },
    Formula {
        name: "Scalable delimiters",
        latex: r"\left\langle\frac{a}{b},\frac{c}{d}\right\rangle",
        typst: r#"lr(chevron.l frac(a,b),frac(c,d) chevron.r)"#,
    },
    Formula {
        name: "Accents and vectors",
        latex: r"\hat{x}+\bar{y}+\vec{v}+\dot{z}+\ddot{q}",
        typst: r#"hat(x)+overline(y)+arrow(v)+dot(z)+dot.double(q)"#,
    },
    Formula {
        name: "Binomial coefficient",
        latex: r"\binom{n}{k}=\frac{n!}{k!(n-k)!}",
        typst: r#"binom(n,k)=frac(n!,k!(n-k)!)"#,
    },
    Formula {
        name: "Product",
        latex: r"\prod_{j=1}^n(1+x_j)",
        typst: r#"product_(j=1)^n (1+x_j)"#,
    },
    Formula {
        name: "Text in math",
        latex: r"\mathbb{P}(X>t)\leq e^{-t}\quad\text{for }t>0",
        typst: r#"bb(P)(X>t)<=e^(-t) quad "for " t>0"#,
    },
    Formula {
        name: "Determinant",
        latex: r"\det\begin{pmatrix}a&b\\c&d\end{pmatrix}=ad-bc",
        typst: r#"det mat(delim: "(",a,b;c,d)=a d-b c"#,
    },
];

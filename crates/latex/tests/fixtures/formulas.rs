pub struct Formula {
    pub name: &'static str,
    pub latex: &'static str,
}

pub const FORMULAS: &[Formula] = &[
    Formula {
        name: "Gaussian integral",
        latex: r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
    },
    Formula {
        name: "Finite sum",
        latex: r"\sum_{k=1}^{n}\frac{1}{k^2}",
    },
    Formula {
        name: "Nested fractions",
        latex: r"\frac{1+\frac{a}{b}}{1-\frac{c}{d}}",
    },
    Formula {
        name: "Quadratic formula",
        latex: r"x=\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
    },
    Formula {
        name: "Two by two matrix",
        latex: r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
    },
    Formula {
        name: "Three by three matrix",
        latex: r"\begin{bmatrix}1&0&0\\0&1&0\\0&0&1\end{bmatrix}",
    },
    Formula {
        name: "Aligned equations",
        latex: r"\begin{aligned}(a+b)^2&=a^2+2ab+b^2\\(a-b)^2&=a^2-2ab+b^2\end{aligned}",
    },
    Formula {
        name: "Piecewise function",
        latex: r"f(x)=\begin{cases}x^2&x\geq0\\-x&x<0\end{cases}",
    },
    Formula {
        name: "Greek alphabet",
        latex: r"\alpha+\beta+\gamma+\delta+\varepsilon+\varphi+\Omega",
    },
    Formula {
        name: "Nested scripts",
        latex: r"a_{i_j}^{n+1}+x^{y^z}+T^{\mu}_{\nu}",
    },
    Formula {
        name: "Limit",
        latex: r"\lim_{x\to0}\frac{\sin x}{x}=1",
    },
    Formula {
        name: "Partial derivative",
        latex: r"\frac{\partial^2 u}{\partial t^2}=c^2\nabla^2u",
    },
    Formula {
        name: "Contour integral",
        latex: r"\oint_C\frac{dz}{z}=2\pi i",
    },
    Formula {
        name: "Set notation",
        latex: r"A=\{x\in\mathbb{R}:x^2<2\}\subseteq\mathbb{R}",
    },
    Formula {
        name: "Scalable delimiters",
        latex: r"\left\langle\frac{a}{b},\frac{c}{d}\right\rangle",
    },
    Formula {
        name: "Accents and vectors",
        latex: r"\hat{x}+\bar{y}+\vec{v}+\dot{z}+\ddot{q}",
    },
    Formula {
        name: "Binomial coefficient",
        latex: r"\binom{n}{k}=\frac{n!}{k!(n-k)!}",
    },
    Formula {
        name: "Product",
        latex: r"\prod_{j=1}^n(1+x_j)",
    },
    Formula {
        name: "Text in math",
        latex: r"\mathbb{P}(X>t)\leq e^{-t}\quad\text{for }t>0",
    },
    Formula {
        name: "Determinant",
        latex: r"\det\begin{pmatrix}a&b\\c&d\end{pmatrix}=ad-bc",
    },
];

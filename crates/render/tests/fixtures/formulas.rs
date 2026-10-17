pub struct Formula {
    pub name: &'static str,
    pub typst: &'static str,
}

pub const FORMULAS: &[Formula] = &[
    Formula {
        name: "Gaussian integral",
        typst: r#"integral_0^oo e^(-x^2) dif x = frac(sqrt(pi), 2)"#,
    },
    Formula {
        name: "Finite sum",
        typst: r#"sum_(k=1)^n frac(1,k^2)"#,
    },
    Formula {
        name: "Nested fractions",
        typst: r#"frac(1+frac(a,b),1-frac(c,d))"#,
    },
    Formula {
        name: "Quadratic formula",
        typst: r#"x=frac(-b plus.minus sqrt(b^2-4a c),2a)"#,
    },
    Formula {
        name: "Two by two matrix",
        typst: r#"mat(delim: "(", a,b;c,d)"#,
    },
    Formula {
        name: "Three by three matrix",
        typst: r#"mat(delim: "[", 1,0,0;0,1,0;0,0,1)"#,
    },
    Formula {
        name: "Aligned equations",
        typst: r#"(a+b)^2 &= a^2+2a b+b^2 \ (a-b)^2 &= a^2-2a b+b^2"#,
    },
    Formula {
        name: "Piecewise function",
        typst: r#"f(x)=cases(x^2 & quad x>=0, -x & quad x<0)"#,
    },
    Formula {
        name: "Greek alphabet",
        typst: r#"alpha+beta+gamma+delta+epsilon+phi+Omega"#,
    },
    Formula {
        name: "Nested scripts",
        typst: r#"a_(i_j)^(n+1)+x^(y^z)+T^mu_nu"#,
    },
    Formula {
        name: "Limit",
        typst: r#"lim_(x arrow.r 0) frac(sin x,x)=1"#,
    },
    Formula {
        name: "Partial derivative",
        typst: r#"frac(partial^2 u,partial t^2)=c^2 nabla^2 u"#,
    },
    Formula {
        name: "Contour integral",
        typst: r#"integral.cont_C frac(dif z,z)=2 pi i"#,
    },
    Formula {
        name: "Set notation",
        typst: r#"A=lr(\{x in RR:x^2<2\}) subset.eq RR"#,
    },
    Formula {
        name: "Scalable delimiters",
        typst: r#"lr(chevron.l frac(a,b),frac(c,d) chevron.r)"#,
    },
    Formula {
        name: "Accents and vectors",
        typst: r#"hat(x)+overline(y)+arrow(v)+dot(z)+dot.double(q)"#,
    },
    Formula {
        name: "Binomial coefficient",
        typst: r#"binom(n,k)=frac(n!,k!(n-k)!)"#,
    },
    Formula {
        name: "Product",
        typst: r#"product_(j=1)^n (1+x_j)"#,
    },
    Formula {
        name: "Text in math",
        typst: r#"bb(P)(X>t)<=e^(-t) quad "for " t>0"#,
    },
    Formula {
        name: "Determinant",
        typst: r#"det mat(delim: "(",a,b;c,d)=a d-b c"#,
    },
];

## 01 Gaussian integral (display)

LaTeX: `\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}`

Typst body: `integral_0^oo e^(-x^2) dif x = frac(sqrt(pi), 2)`

## 01 Gaussian integral (inline)

LaTeX: `\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}`

Typst body: `integral_0^oo e^(-x^2) dif x = frac(sqrt(pi), 2)`

## 02 Finite sum (display)

LaTeX: `\sum_{k=1}^{n}\frac{1}{k^2}`

Typst body: `sum_(k=1)^n frac(1,k^2)`

## 02 Finite sum (inline)

LaTeX: `\sum_{k=1}^{n}\frac{1}{k^2}`

Typst body: `sum_(k=1)^n frac(1,k^2)`

## 03 Nested fractions (display)

LaTeX: `\frac{1+\frac{a}{b}}{1-\frac{c}{d}}`

Typst body: `frac(1+frac(a,b),1-frac(c,d))`

## 03 Nested fractions (inline)

LaTeX: `\frac{1+\frac{a}{b}}{1-\frac{c}{d}}`

Typst body: `frac(1+frac(a,b),1-frac(c,d))`

## 04 Quadratic formula (display)

LaTeX: `x=\frac{-b\pm\sqrt{b^2-4ac}}{2a}`

Typst body: `x=frac(-b plus.minus sqrt(b^2-4a c),2a)`

## 04 Quadratic formula (inline)

LaTeX: `x=\frac{-b\pm\sqrt{b^2-4ac}}{2a}`

Typst body: `x=frac(-b plus.minus sqrt(b^2-4a c),2a)`

## 05 Two by two matrix (display)

LaTeX: `\begin{pmatrix}a&b\\c&d\end{pmatrix}`

Typst body: `mat(delim: "(", a,b;c,d)`

## 05 Two by two matrix (inline)

LaTeX: `\begin{pmatrix}a&b\\c&d\end{pmatrix}`

Typst body: `mat(delim: "(", a,b;c,d)`

## 06 Three by three matrix (display)

LaTeX: `\begin{bmatrix}1&0&0\\0&1&0\\0&0&1\end{bmatrix}`

Typst body: `mat(delim: "[", 1,0,0;0,1,0;0,0,1)`

## 06 Three by three matrix (inline)

LaTeX: `\begin{bmatrix}1&0&0\\0&1&0\\0&0&1\end{bmatrix}`

Typst body: `mat(delim: "[", 1,0,0;0,1,0;0,0,1)`

## 07 Aligned equations (display)

LaTeX: `\begin{aligned}(a+b)^2&=a^2+2ab+b^2\\(a-b)^2&=a^2-2ab+b^2\end{aligned}`

Typst body: `(a+b)^2 &= a^2+2a b+b^2 \ (a-b)^2 &= a^2-2a b+b^2`

## 07 Aligned equations (inline)

LaTeX: `\begin{aligned}(a+b)^2&=a^2+2ab+b^2\\(a-b)^2&=a^2-2ab+b^2\end{aligned}`

Typst body: `(a+b)^2 &= a^2+2a b+b^2 \ (a-b)^2 &= a^2-2a b+b^2`

## 08 Piecewise function (display)

LaTeX: `f(x)=\begin{cases}x^2&x\geq0\\-x&x<0\end{cases}`

Typst body: `f(x)=cases(x^2 & quad x>=0, -x & quad x<0)`

## 08 Piecewise function (inline)

LaTeX: `f(x)=\begin{cases}x^2&x\geq0\\-x&x<0\end{cases}`

Typst body: `f(x)=cases(x^2 & quad x>=0, -x & quad x<0)`

## 09 Greek alphabet (display)

LaTeX: `\alpha+\beta+\gamma+\delta+\varepsilon+\varphi+\Omega`

Typst body: `alpha+beta+gamma+delta+epsilon+phi+Omega`

## 09 Greek alphabet (inline)

LaTeX: `\alpha+\beta+\gamma+\delta+\varepsilon+\varphi+\Omega`

Typst body: `alpha+beta+gamma+delta+epsilon+phi+Omega`

## 10 Nested scripts (display)

LaTeX: `a_{i_j}^{n+1}+x^{y^z}+T^{\mu}_{\nu}`

Typst body: `a_(i_j)^(n+1)+x^(y^z)+T^mu_nu`

## 10 Nested scripts (inline)

LaTeX: `a_{i_j}^{n+1}+x^{y^z}+T^{\mu}_{\nu}`

Typst body: `a_(i_j)^(n+1)+x^(y^z)+T^mu_nu`

## 11 Limit (display)

LaTeX: `\lim_{x\to0}\frac{\sin x}{x}=1`

Typst body: `lim_(x arrow.r 0) frac(sin x,x)=1`

## 11 Limit (inline)

LaTeX: `\lim_{x\to0}\frac{\sin x}{x}=1`

Typst body: `lim_(x arrow.r 0) frac(sin x,x)=1`

## 12 Partial derivative (display)

LaTeX: `\frac{\partial^2 u}{\partial t^2}=c^2\nabla^2u`

Typst body: `frac(partial^2 u,partial t^2)=c^2 nabla^2 u`

## 12 Partial derivative (inline)

LaTeX: `\frac{\partial^2 u}{\partial t^2}=c^2\nabla^2u`

Typst body: `frac(partial^2 u,partial t^2)=c^2 nabla^2 u`

## 13 Contour integral (display)

LaTeX: `\oint_C\frac{dz}{z}=2\pi i`

Typst body: `integral.cont_C frac(dif z,z)=2 pi i`

## 13 Contour integral (inline)

LaTeX: `\oint_C\frac{dz}{z}=2\pi i`

Typst body: `integral.cont_C frac(dif z,z)=2 pi i`

## 14 Set notation (display)

LaTeX: `A=\{x\in\mathbb{R}:x^2<2\}\subseteq\mathbb{R}`

Typst body: `A=lr(\{x in RR:x^2<2\}) subset.eq RR`

## 14 Set notation (inline)

LaTeX: `A=\{x\in\mathbb{R}:x^2<2\}\subseteq\mathbb{R}`

Typst body: `A=lr(\{x in RR:x^2<2\}) subset.eq RR`

## 15 Scalable delimiters (display)

LaTeX: `\left\langle\frac{a}{b},\frac{c}{d}\right\rangle`

Typst body: `lr(chevron.l frac(a,b),frac(c,d) chevron.r)`

## 15 Scalable delimiters (inline)

LaTeX: `\left\langle\frac{a}{b},\frac{c}{d}\right\rangle`

Typst body: `lr(chevron.l frac(a,b),frac(c,d) chevron.r)`

## 16 Accents and vectors (display)

LaTeX: `\hat{x}+\bar{y}+\vec{v}+\dot{z}+\ddot{q}`

Typst body: `hat(x)+overline(y)+arrow(v)+dot(z)+dot.double(q)`

## 16 Accents and vectors (inline)

LaTeX: `\hat{x}+\bar{y}+\vec{v}+\dot{z}+\ddot{q}`

Typst body: `hat(x)+overline(y)+arrow(v)+dot(z)+dot.double(q)`

## 17 Binomial coefficient (display)

LaTeX: `\binom{n}{k}=\frac{n!}{k!(n-k)!}`

Typst body: `binom(n,k)=frac(n!,k!(n-k)!)`

## 17 Binomial coefficient (inline)

LaTeX: `\binom{n}{k}=\frac{n!}{k!(n-k)!}`

Typst body: `binom(n,k)=frac(n!,k!(n-k)!)`

## 18 Product (display)

LaTeX: `\prod_{j=1}^n(1+x_j)`

Typst body: `product_(j=1)^n (1+x_j)`

## 18 Product (inline)

LaTeX: `\prod_{j=1}^n(1+x_j)`

Typst body: `product_(j=1)^n (1+x_j)`

## 19 Text in math (display)

LaTeX: `\mathbb{P}(X>t)\leq e^{-t}\quad\text{for }t>0`

Typst body: `bb(P)(X>t)<=e^(-t) quad "for " t>0`

## 19 Text in math (inline)

LaTeX: `\mathbb{P}(X>t)\leq e^{-t}\quad\text{for }t>0`

Typst body: `bb(P)(X>t)<=e^(-t) quad "for " t>0`

## 20 Determinant (display)

LaTeX: `\det\begin{pmatrix}a&b\\c&d\end{pmatrix}=ad-bc`

Typst body: `det mat(delim: "(",a,b;c,d)=a d-b c`

## 20 Determinant (inline)

LaTeX: `\det\begin{pmatrix}a&b\\c&d\end{pmatrix}=ad-bc`

Typst body: `det mat(delim: "(",a,b;c,d)=a d-b c`


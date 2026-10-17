/// Spec items that cannot render in math mode with the generic or in-context samples, with the reason.
pub const SPEC_ALLOWLIST: &[(&str, &str)] = &[
    (
        "@ifstar",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "AtBeginDocument",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "AtEndDocument",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "AtEndOfClass",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "AtEndOfPackage",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "CheckCommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "CheckCommand*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "CurrentOption",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "DeclareOption",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "DeclareOption*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "DeclareRobustCommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "DeclareRobustCommand*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "DeclareTextCommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "DeclareTextCommandDefault",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ExecuteOptions",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    ("Huge", "size declaration: nothing to draw on its own"),
    (
        "IfFileExists",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "InputIfFileExists",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    ("LARGE", "size declaration: nothing to draw on its own"),
    ("Large", "size declaration: nothing to draw on its own"),
    (
        "LoadClass",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "LoadClassWithOptions",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "PassOptionsToClass",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "PassOptionsToPackage",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ProcessOptions",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ProvideTextCommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ProvideTextCommandDefault",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ProvidesClass",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ProvidesFile",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "RequirePackage",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "RequirePackageWithOptions",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "begingroup",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "bottomrule",
        "text-mode only (table rule, list item, file access or length macro)",
    ),
    (
        "caption",
        "label, numbering or float/table command the converter drops in math",
    ),
    (
        "centering",
        "label, numbering or float/table command the converter drops in math",
    ),
    (
        "cite",
        "needs a label or bibliography defined in the document",
    ),
    (
        "cr",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "def",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "documentclass",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "eqref",
        "needs a label or bibliography defined in the document",
    ),
    (
        "expandafter",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "footnotesize",
        "size declaration: nothing to draw on its own",
    ),
    (
        "hline",
        "label, numbering or float/table command the converter drops in math",
    ),
    ("huge", "size declaration: nothing to draw on its own"),
    (
        "if",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifdim",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "iffalse",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifhbox",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifhmode",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifinner",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifmmode",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifnum",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifodd",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "iftrue",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifvbox",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifvmode",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifvoid",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ifx",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ignorespaces",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "ignorespacesafterend",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "includegraphics",
        "text-mode only (table rule, list item, file access or length macro)",
    ),
    (
        "item",
        "text-mode only (table rule, list item, file access or length macro)",
    ),
    (
        "label",
        "label, numbering or float/table command the converter drops in math",
    ),
    ("large", "size declaration: nothing to draw on its own"),
    (
        "midrule",
        "text-mode only (table rule, list item, file access or length macro)",
    ),
    (
        "newcommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newcommand*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newcounter",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newenvironment",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newenvironment*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newfont",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newlength",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newline",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "newsavebox",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "newtheorem",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "noexpand",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "nonumber",
        "label, numbering or float/table command the converter drops in math",
    ),
    ("normalsize", "size declaration: nothing to draw on its own"),
    (
        "notag",
        "label, numbering or float/table command the converter drops in math",
    ),
    (
        "providecommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "providecommand*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "ref",
        "needs a label or bibliography defined in the document",
    ),
    (
        "relax",
        "TeX or LaTeX primitive or ignored token: nothing to draw",
    ),
    (
        "renewcommand",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "renewcommand*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "renewenvironment",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    (
        "renewenvironment*",
        "preamble, macro-definition or conditional command: converter emits nothing to draw",
    ),
    ("scriptsize", "size declaration: nothing to draw on its own"),
    ("small", "size declaration: nothing to draw on its own"),
    (
        "tabular",
        "text-mode environment; converter output is malformed inside math",
    ),
    (
        "tag",
        "label, numbering or float/table command the converter drops in math",
    ),
    (
        "textwidth",
        "text-mode only (table rule, list item, file access or length macro)",
    ),
    ("tiny", "size declaration: nothing to draw on its own"),
    (
        "toprule",
        "text-mode only (table rule, list item, file access or length macro)",
    ),
    (
        "vline",
        "label, numbering or float/table command the converter drops in math",
    ),
];

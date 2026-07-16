// best-effort TeX -> Unicode: greek, sub/superscripts, common operators.
// Unmappable scripts fall back to ^(...) / _(...) so nothing is ever dropped.

pub fn tex_to_unicode(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    conv(&chars, &mut i, false)
}

fn conv(s: &[char], i: &mut usize, stop_at_brace: bool) -> String {
    let mut out = String::new();
    while *i < s.len() {
        match s[*i] {
            '}' if stop_at_brace => {
                *i += 1;
                return out;
            }
            '}' => *i += 1,
            '{' => {
                *i += 1;
                out.push_str(&conv(s, i, true));
            }
            '\\' => {
                *i += 1;
                out.push_str(&command(s, i));
            }
            '^' => {
                *i += 1;
                script(s, i, &mut out, true);
            }
            '_' => {
                *i += 1;
                script(s, i, &mut out, false);
            }
            '~' => {
                *i += 1;
                out.push(' ');
            }
            '&' => *i += 1,
            c => {
                *i += 1;
                out.push(c);
            }
        }
    }
    out
}

fn unit(s: &[char], i: &mut usize) -> String {
    if *i >= s.len() {
        return String::new();
    }
    match s[*i] {
        '{' => {
            *i += 1;
            conv(s, i, true)
        }
        '\\' => {
            *i += 1;
            command(s, i)
        }
        c => {
            *i += 1;
            c.to_string()
        }
    }
}

fn script(s: &[char], i: &mut usize, out: &mut String, sup: bool) {
    let content = unit(s, i);
    if content.is_empty() {
        return;
    }
    let mapped: Option<String> = content
        .chars()
        .map(|c| map_script(c, sup).map(String::from))
        .collect();
    match mapped {
        Some(m) => out.push_str(&m),
        None => {
            out.push(if sup { '^' } else { '_' });
            if content.chars().count() > 1 {
                out.push('(');
                out.push_str(&content);
                out.push(')');
            } else {
                out.push_str(&content);
            }
        }
    }
}

fn command(s: &[char], i: &mut usize) -> String {
    let start = *i;
    while *i < s.len() && s[*i].is_ascii_alphabetic() {
        *i += 1;
    }
    if *i == start {
        let Some(&c) = s.get(*i) else {
            return String::new();
        };
        *i += 1;
        return match c {
            ',' | ';' | ':' | ' ' => " ".to_string(),
            '!' => String::new(),
            '\\' => "\n".to_string(),
            '|' => "‖".to_string(),
            c => c.to_string(),
        };
    }
    let name: String = s[start..*i].iter().collect();
    match name.as_str() {
        "frac" | "dfrac" | "tfrac" => {
            let a = unit(s, i);
            let b = unit(s, i);
            format!("{}/{}", paren(&a), paren(&b))
        }
        "sqrt" => {
            let a = unit(s, i);
            format!("√{}", paren(&a))
        }
        "text" | "textrm" | "textit" | "textbf" | "mathrm" | "mathit" | "mathbf"
        | "mathsf" | "mathcal" | "mathfrak" | "operatorname" | "boldsymbol" | "bm" => {
            unit(s, i)
        }
        "mathbb" => unit(s, i).chars().map(blackboard).collect(),
        "hat" | "bar" | "tilde" | "vec" | "dot" | "ddot" | "overline" => {
            accent(&unit(s, i), &name)
        }
        "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl"
        | "Bigr" => {
            if s.get(*i) == Some(&'.') {
                *i += 1;
            }
            String::new()
        }
        "begin" | "end" | "label" | "tag" => {
            unit(s, i);
            String::new()
        }
        "nonumber" | "notag" | "displaystyle" | "limits" => String::new(),
        "quad" => "  ".to_string(),
        "qquad" => "    ".to_string(),
        _ => symbol(&name).map(String::from).unwrap_or(name),
    }
}

fn paren(x: &str) -> String {
    if x.chars().count() <= 1 {
        x.to_string()
    } else {
        format!("({x})")
    }
}

fn accent(x: &str, name: &str) -> String {
    let mark = match name {
        "hat" => '\u{0302}',
        "bar" | "overline" => '\u{0304}',
        "tilde" => '\u{0303}',
        "vec" => '\u{20D7}',
        "dot" => '\u{0307}',
        "ddot" => '\u{0308}',
        _ => return x.to_string(),
    };
    let mut cs = x.chars();
    match cs.next() {
        Some(first) => {
            let mut out = String::new();
            out.push(first);
            out.push(mark);
            out.extend(cs);
            out
        }
        None => String::new(),
    }
}

fn blackboard(c: char) -> char {
    match c {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        'E' => '𝔼',
        'A'..='Z' => {
            char::from_u32(0x1D538 + (c as u32 - 'A' as u32)).unwrap_or(c)
        }
        _ => c,
    }
}

fn map_script(c: char, sup: bool) -> Option<char> {
    if sup {
        match c {
            '0' => Some('⁰'),
            '1' => Some('¹'),
            '2' => Some('²'),
            '3' => Some('³'),
            '4' => Some('⁴'),
            '5' => Some('⁵'),
            '6' => Some('⁶'),
            '7' => Some('⁷'),
            '8' => Some('⁸'),
            '9' => Some('⁹'),
            '+' => Some('⁺'),
            '-' | '−' => Some('⁻'),
            '=' => Some('⁼'),
            '(' => Some('⁽'),
            ')' => Some('⁾'),
            'a' => Some('ᵃ'),
            'b' => Some('ᵇ'),
            'c' => Some('ᶜ'),
            'd' => Some('ᵈ'),
            'e' => Some('ᵉ'),
            'f' => Some('ᶠ'),
            'g' => Some('ᵍ'),
            'h' => Some('ʰ'),
            'i' => Some('ⁱ'),
            'j' => Some('ʲ'),
            'k' => Some('ᵏ'),
            'l' => Some('ˡ'),
            'm' => Some('ᵐ'),
            'n' => Some('ⁿ'),
            'o' => Some('ᵒ'),
            'p' => Some('ᵖ'),
            'r' => Some('ʳ'),
            's' => Some('ˢ'),
            't' => Some('ᵗ'),
            'u' => Some('ᵘ'),
            'v' => Some('ᵛ'),
            'w' => Some('ʷ'),
            'x' => Some('ˣ'),
            'y' => Some('ʸ'),
            'z' => Some('ᶻ'),
            'T' => Some('ᵀ'),
            _ => None,
        }
    } else {
        match c {
            '0' => Some('₀'),
            '1' => Some('₁'),
            '2' => Some('₂'),
            '3' => Some('₃'),
            '4' => Some('₄'),
            '5' => Some('₅'),
            '6' => Some('₆'),
            '7' => Some('₇'),
            '8' => Some('₈'),
            '9' => Some('₉'),
            '+' => Some('₊'),
            '-' | '−' => Some('₋'),
            '=' => Some('₌'),
            '(' => Some('₍'),
            ')' => Some('₎'),
            'a' => Some('ₐ'),
            'e' => Some('ₑ'),
            'h' => Some('ₕ'),
            'i' => Some('ᵢ'),
            'j' => Some('ⱼ'),
            'k' => Some('ₖ'),
            'l' => Some('ₗ'),
            'm' => Some('ₘ'),
            'n' => Some('ₙ'),
            'o' => Some('ₒ'),
            'p' => Some('ₚ'),
            'r' => Some('ᵣ'),
            's' => Some('ₛ'),
            't' => Some('ₜ'),
            'u' => Some('ᵤ'),
            'v' => Some('ᵥ'),
            'x' => Some('ₓ'),
            _ => None,
        }
    }
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "φ",
        "varphi" => "ϕ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        "pm" => "±",
        "mp" => "∓",
        "times" => "×",
        "div" => "÷",
        "cdot" => "·",
        "ast" => "∗",
        "star" => "⋆",
        "circ" => "∘",
        "bullet" => "•",
        "oplus" => "⊕",
        "ominus" => "⊖",
        "otimes" => "⊗",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "propto" => "∝",
        "ll" => "≪",
        "gg" => "≫",
        "prec" => "≺",
        "succ" => "≻",
        "to" | "rightarrow" => "→",
        "leftarrow" | "gets" => "←",
        "leftrightarrow" => "↔",
        "Rightarrow" | "implies" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "supset" => "⊃",
        "subseteq" => "⊆",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "emptyset" | "varnothing" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "neg" | "lnot" => "¬",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "angle" => "∠",
        "perp" => "⊥",
        "parallel" => "∥",
        "mid" => "∣",
        "nmid" => "∤",
        "dots" | "ldots" | "dotsc" => "…",
        "cdots" | "dotsb" => "⋯",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "prime" => "′",
        "degree" => "°",
        "ell" => "ℓ",
        "hbar" => "ℏ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "wp" => "℘",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "Vert" => "‖",
        "vert" => "∣",
        _ => return None,
    })
}

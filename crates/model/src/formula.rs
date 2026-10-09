//! A small, safe expression evaluator for Markups List formula columns (Revu: Manage Columns >
//! Formula, e.g. `Length * UnitCost`).
//!
//! Grammar (no assignment, no loops, nothing outside this file is called):
//!
//! ```text
//! expr    := term (('+' | '-') term)*
//! term    := unary (('*' | '/' | '%') unary)*        % = remainder
//! unary   := ('-' | '+') unary | power
//! power   := primary ('^' unary)?                   right associative: 2^3^2 = 2^9
//! primary := number | name | name '(' args ')' | '(' expr ')' | '[' any text ']'
//! number  := 12  1.5  .5  1e3   (1,250.5 is not a number)
//! name    := letters, digits, '_' and '.', starting with a letter or '_'
//! ```
//!
//! `[Unit Cost]` names a column whose header has spaces. Constants: `pi`, `e`. Functions:
//! `sqrt abs sin cos tan asin acos atan log` (base 10) `ln exp floor ceil` (or `ceiling`) `round(x)
//! round(x, digits) min(a, ...) max(a, ...) pow(a, b) if(c, a, b)`. Names are case-insensitive.
//! Unknown names, division by zero and non-finite results are errors.

/// Longest formula accepted (keeps parse and evaluation recursion bounded).
pub const MAX_FORMULA_CHARS: usize = 2000;
const MAX_DEPTH: usize = 200;

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f64),
    Var(String),
    Neg(Box<Node>),
    Bin(char, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

/// A parsed formula. Parse once, evaluate per markup.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Formula {
    root: Option<Node>,
    error: String,
}

/// Variable lookup: the name as written (without brackets) to a value, `None` if unknown.
pub type Resolver<'a> = dyn FnMut(&str) -> Option<f64> + 'a;

impl Formula {
    /// Parse `text`; on failure [`Formula::ok`] is false and [`Formula::error`] says where.
    pub fn parse(text: &str) -> Formula {
        if text.trim().is_empty() {
            return Formula {
                root: None,
                error: "empty formula".into(),
            };
        }
        if text.chars().count() > MAX_FORMULA_CHARS {
            return Formula {
                root: None,
                error: format!("formula longer than {MAX_FORMULA_CHARS} characters"),
            };
        }
        let mut p = Parser {
            s: text.chars().collect(),
            i: 0,
            depth: 0,
        };
        match p.parse_all() {
            Ok(n) => Formula {
                root: Some(n),
                error: String::new(),
            },
            Err(e) => Formula { root: None, error: e },
        }
    }

    pub fn ok(&self) -> bool {
        self.root.is_some()
    }

    pub fn error(&self) -> &str {
        &self.error
    }

    /// Every variable name the expression uses (constants excluded), in order of appearance.
    pub fn names(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(r) = &self.root {
            collect(r, &mut out);
        }
        out
    }

    /// Evaluate with `vars` resolving column names.
    pub fn eval(&self, vars: &mut Resolver<'_>) -> Result<f64, String> {
        let Some(root) = &self.root else {
            return Err(self.error.clone());
        };
        let v = eval_node(root, vars)?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err("result is not a number".into())
        }
    }
}

/// Parse and evaluate in one step.
pub fn eval_formula(text: &str, vars: &mut Resolver<'_>) -> Result<f64, String> {
    Formula::parse(text).eval(vars)
}

/// A column name normalised for formula lookup: lower case, no white space or underscores.
pub fn formula_key(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

struct Parser {
    s: Vec<char>,
    i: usize,
    depth: usize,
}

type PResult = Result<Node, String>;

impl Parser {
    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at position {}", self.i + 1))
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn skip(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn parse_all(&mut self) -> PResult {
        let n = self.expr()?;
        self.skip();
        if let Some(c) = self.peek() {
            return self.fail(&format!("unexpected '{c}'"));
        }
        Ok(n)
    }

    fn expr(&mut self) -> PResult {
        let mut l = self.term()?;
        loop {
            if self.eat('+') {
                l = Node::Bin('+', Box::new(l), Box::new(self.term()?));
            } else if self.eat('-') {
                l = Node::Bin('-', Box::new(l), Box::new(self.term()?));
            } else {
                return Ok(l);
            }
        }
    }

    fn term(&mut self) -> PResult {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat('*') {
                '*'
            } else if self.eat('/') {
                '/'
            } else if self.eat('%') {
                '%'
            } else {
                return Ok(l);
            };
            l = Node::Bin(op, Box::new(l), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> PResult {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.fail("expression too deep");
        }
        let r = if self.eat('-') {
            self.unary().map(|n| Node::Neg(Box::new(n)))
        } else if self.eat('+') {
            self.unary()
        } else {
            self.power()
        };
        self.depth -= 1;
        r
    }

    fn power(&mut self) -> PResult {
        let base = self.primary()?;
        if self.eat('^') {
            return Ok(Node::Bin('^', Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn primary(&mut self) -> PResult {
        self.skip();
        let Some(c) = self.peek() else {
            return self.fail("unexpected end");
        };
        if c == '(' {
            self.i += 1;
            let n = self.expr()?;
            if !self.eat(')') {
                return self.fail("missing ')'");
            }
            return Ok(n);
        }
        if c == '[' {
            let start = self.i + 1;
            let Some(len) = self
                .s
                .get(start..)
                .and_then(|rest| rest.iter().position(|&ch| ch == ']'))
            else {
                return self.fail("missing ']'");
            };
            let name: String = self.s.get(start..start + len).unwrap_or_default().iter().collect();
            self.i = start + len + 1;
            return Ok(Node::Var(name));
        }
        if c.is_ascii_digit() || c == '.' {
            return self.number();
        }
        if c.is_alphabetic() || c == '_' {
            let st = self.i;
            while self
                .peek()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '.')
            {
                self.i += 1;
            }
            let name: String = self.s.get(st..self.i).unwrap_or_default().iter().collect();
            if self.eat('(') {
                let fname = name.to_lowercase();
                let mut args = Vec::new();
                if !self.eat(')') {
                    loop {
                        args.push(self.expr()?);
                        if !self.eat(',') {
                            break;
                        }
                    }
                    if !self.eat(')') {
                        return self.fail(&format!("missing ')' after arguments of {name}"));
                    }
                }
                self.check_call(&fname, args.len())?;
                return Ok(Node::Call(fname, args));
            }
            return Ok(Node::Var(name));
        }
        self.fail(&format!("unexpected '{c}'"))
    }

    fn number(&mut self) -> PResult {
        let st = self.i;
        let (mut dot, mut digits) = (false, false);
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                digits = true;
            } else if c == '.' && !dot {
                dot = true;
            } else {
                break;
            }
            self.i += 1;
        }
        if !digits {
            return self.fail("bad number");
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let mut k = self.i + 1;
            if matches!(self.s.get(k), Some('+' | '-')) {
                k += 1;
            }
            if self.s.get(k).is_some_and(char::is_ascii_digit) {
                self.i = k;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
            }
        }
        let text: String = self.s.get(st..self.i).unwrap_or_default().iter().collect();
        match text.parse::<f64>() {
            Ok(v) => Ok(Node::Num(v)),
            Err(_) => self.fail("bad number"),
        }
    }

    fn check_call(&self, name: &str, k: usize) -> Result<(), String> {
        const ONE: &[&str] = &[
            "sqrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "log", "ln", "exp", "floor", "ceil", "ceiling",
        ];
        let ok = match name {
            n if ONE.contains(&n) => {
                if k != 1 {
                    return self.fail(&format!("{name}() takes 1 argument"));
                }
                true
            }
            "round" => k == 1 || k == 2,
            "min" | "max" => k >= 1,
            "pow" => k == 2,
            "if" => k == 3,
            _ => return self.fail(&format!("unknown function {name}()")),
        };
        if ok {
            Ok(())
        } else {
            let want = match name {
                "round" => "1 or 2 arguments",
                "min" | "max" => "at least 1 argument",
                "pow" => "2 arguments",
                _ => "3 arguments",
            };
            self.fail(&format!("{name}() takes {want}"))
        }
    }
}

fn arg(a: &[f64], i: usize) -> f64 {
    a.get(i).copied().unwrap_or(0.0)
}

fn eval_node(n: &Node, vars: &mut Resolver<'_>) -> Result<f64, String> {
    match n {
        Node::Num(v) => Ok(*v),
        Node::Var(name) => {
            let k = name.to_lowercase();
            if k == "pi" {
                return Ok(std::f64::consts::PI);
            }
            if k == "e" {
                return Ok(std::f64::consts::E);
            }
            vars(name).ok_or_else(|| format!("unknown column '{name}'"))
        }
        Node::Neg(x) => Ok(-eval_node(x, vars)?),
        Node::Bin(op, x, y) => {
            let a = eval_node(x, vars)?;
            let b = eval_node(y, vars)?;
            match op {
                '+' => Ok(a + b),
                '-' => Ok(a - b),
                '*' => Ok(a * b),
                '/' | '%' if b == 0.0 => Err("division by zero".into()),
                '/' => Ok(a / b),
                '%' => Ok(a % b),
                '^' => Ok(a.powf(b)),
                _ => Err("bad operator".into()),
            }
        }
        Node::Call(f, kids) => {
            let mut a = Vec::with_capacity(kids.len());
            for k in kids {
                a.push(eval_node(k, vars)?);
            }
            let x = arg(&a, 0);
            Ok(match f.as_str() {
                "sqrt" => x.sqrt(),
                "abs" => x.abs(),
                "sin" => x.sin(),
                "cos" => x.cos(),
                "tan" => x.tan(),
                "asin" => x.asin(),
                "acos" => x.acos(),
                "atan" => x.atan(),
                "log" => x.log10(),
                "ln" => x.ln(),
                "exp" => x.exp(),
                "floor" => x.floor(),
                "ceil" | "ceiling" => x.ceil(),
                "round" => {
                    let m = if a.len() > 1 {
                        10f64.powf(arg(&a, 1).round())
                    } else {
                        1.0
                    };
                    (x * m).round() / m
                }
                "min" => a.iter().copied().fold(x, f64::min),
                "max" => a.iter().copied().fold(x, f64::max),
                "pow" => x.powf(arg(&a, 1)),
                "if" => {
                    if x != 0.0 {
                        arg(&a, 1)
                    } else {
                        arg(&a, 2)
                    }
                }
                _ => return Err(format!("unknown function {f}")),
            })
        }
    }
}

fn collect(n: &Node, out: &mut Vec<String>) {
    match n {
        Node::Var(name) => {
            let k = name.to_lowercase();
            if k != "pi" && k != "e" {
                out.push(name.clone());
            }
        }
        Node::Num(_) => {}
        Node::Neg(x) => collect(x, out),
        Node::Bin(_, x, y) => {
            collect(x, out);
            collect(y, out);
        }
        Node::Call(_, kids) => kids.iter().for_each(|k| collect(k, out)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(n: &str) -> Option<f64> {
        match formula_key(n).as_str() {
            "length" => Some(12.5),
            "unitcost" => Some(4.0),
            "area" => Some(100.0),
            _ => None,
        }
    }

    fn ev(s: &str) -> Result<f64, String> {
        eval_formula(s, &mut vars)
    }

    fn near(s: &str, want: f64) {
        match ev(s) {
            Ok(v) => assert!((v - want).abs() < 1e-9, "{s}: got {v} want {want}"),
            Err(e) => panic!("{s}: {e}"),
        }
    }

    #[test]
    fn formula_arithmetic_and_precedence() {
        near("1 + 2 * 3", 7.0);
        near("(1 + 2) * 3", 9.0);
        near("2 ^ 3 ^ 2", 512.0);
        near("-2 ^ 2", -4.0);
        near("10 % 4", 2.0);
        near("1.5e2 + .5", 150.5);
    }

    #[test]
    fn formula_column_references() {
        near("Length * UnitCost", 50.0);
        near("[Unit Cost] * length", 50.0);
        near("unit_cost * 2", 8.0);
    }

    #[test]
    fn formula_functions_and_constants() {
        near("sqrt(Area) + abs(-1)", 11.0);
        near("round(pi, 2) * 100", 314.0);
        near("max(1, Length, 3) - min(4, 2)", 10.5);
        near("if(Length - 12.5, 1, 2)", 2.0);
        near("log(1000) + ln(e)", 4.0);
        near("pow(2, 10) + floor(1.7) + ceil(1.2)", 1027.0);
        near("ceiling(Length)", 13.0);
    }

    #[test]
    fn formula_errors_never_panic() {
        assert!(ev("1 / 0").is_err_and(|e| e.contains("zero")));
        assert!(ev("Nope * 2").is_err_and(|e| e.contains("Nope")));
        assert!(ev("sqrt(-1)").is_err());
        for bad in [
            "1 +",
            "(1 + 2",
            "foo(1)",
            "sqrt(1, 2)",
            "1 2",
            "",
            "system(\"x\")",
            "[abc",
            "pow(1)",
        ] {
            assert!(!Formula::parse(bad).ok(), "{bad} should not parse");
        }
        let f = Formula::parse("Length * [Unit Cost] + pi");
        assert!(f.ok());
        assert_eq!(f.names(), vec!["Length".to_string(), "Unit Cost".to_string()]);
        let deep = format!("{}1{}", "(".repeat(300), ")".repeat(300));
        assert!(!Formula::parse(&deep).ok());
        let long = "1+".repeat(5000) + "1";
        assert!(!Formula::parse(&long).ok());
        let chain = "1+".repeat(900) + "1";
        near(&chain, 901.0);
    }
}

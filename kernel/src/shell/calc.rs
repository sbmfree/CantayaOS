// CantayaOS Shell — Expression Evaluator (calc)

extern crate alloc;

use alloc::vec::Vec;

/// Simple recursive descent expression evaluator for integer arithmetic.
pub(crate) fn eval_expr(input: &str) -> Option<i64> {
    let tokens = tokenize_expr(input)?;
    let mut pos = 0;
    let result = parse_add_sub(&tokens, &mut pos)?;
    if pos == tokens.len() {
        Some(result)
    } else {
        None // leftover tokens
    }
}

#[derive(Debug, Clone)]
enum Token {
    Num(i64),
    Op(u8), // b'+', b'-', b'*', b'/', b'%'
    LParen,
    RParen,
}

fn tokenize_expr(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' => { i += 1; }
            b'+' | b'-' | b'*' | b'/' | b'%' => {
                // Handle negative numbers: if '-' is at start or after operator/lparen
                if bytes[i] == b'-' {
                    let is_unary = tokens.is_empty() ||
                        matches!(tokens.last(), Some(Token::Op(_)) | Some(Token::LParen));
                    if is_unary {
                        // Parse as negative number
                        i += 1;
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
                        if i == start { return None; }
                        let num_str = core::str::from_utf8(&bytes[start..i]).ok()?;
                        let n: i64 = num_str.parse().ok()?;
                        tokens.push(Token::Num(-n));
                        continue;
                    }
                }
                tokens.push(Token::Op(bytes[i]));
                i += 1;
            }
            b'(' => { tokens.push(Token::LParen); i += 1; }
            b')' => { tokens.push(Token::RParen); i += 1; }
            b'0'..=b'9' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
                let num_str = core::str::from_utf8(&bytes[start..i]).ok()?;
                tokens.push(Token::Num(num_str.parse().ok()?));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

fn parse_add_sub(tokens: &[Token], pos: &mut usize) -> Option<i64> {
    let mut left = parse_mul_div(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Op(b'+') => { *pos += 1; left += parse_mul_div(tokens, pos)?; }
            Token::Op(b'-') => { *pos += 1; left -= parse_mul_div(tokens, pos)?; }
            _ => break,
        }
    }
    Some(left)
}

fn parse_mul_div(tokens: &[Token], pos: &mut usize) -> Option<i64> {
    let mut left = parse_primary(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Op(b'*') => { *pos += 1; left *= parse_primary(tokens, pos)?; }
            Token::Op(b'/') => {
                *pos += 1;
                let right = parse_primary(tokens, pos)?;
                if right == 0 { return None; } // division by zero
                left /= right;
            }
            Token::Op(b'%') => {
                *pos += 1;
                let right = parse_primary(tokens, pos)?;
                if right == 0 { return None; }
                left %= right;
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Option<i64> {
    if *pos >= tokens.len() { return None; }
    match &tokens[*pos] {
        Token::Num(n) => { let v = *n; *pos += 1; Some(v) }
        Token::LParen => {
            *pos += 1;
            let result = parse_add_sub(tokens, pos)?;
            if *pos < tokens.len() && matches!(tokens[*pos], Token::RParen) {
                *pos += 1;
                Some(result)
            } else {
                None // missing closing paren
            }
        }
        _ => None,
    }
}

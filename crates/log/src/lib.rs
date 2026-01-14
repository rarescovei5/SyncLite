use chrono::Utc;
use proc_macro::TokenStream;
use quote::quote;
use syn::{Expr, ExprLit, Lit, LitStr, parse_macro_input, spanned::Spanned};

mod ast;
use ast::LogArgs;

fn generate_log(input: LogArgs, is_error: bool) -> TokenStream {
    let LogArgs {
        preset,
        indent,
        msg,
        args,
    } = input;

    let macro_name = if is_error {
        quote!(eprintln)
    } else {
        quote!(println)
    };

    let prefix = match preset {
        Some(p) => {
            let s = p.to_string();
            match s.as_str() {
                "info" => "ℹ️  ".to_owned(),
                "wrench" => "🔧 ".to_owned(),
                "error" => "❌ ".to_owned(),
                "warning" => "⚠️ ".to_owned(),
                "success" => "✅ ".to_owned(),
                "question" => "🤔 ".to_owned(),
                "log" => {
                    let time_str = Utc::now().time().to_string();
                    let time = &time_str[..13];

                    format!("\x1b[30m[{}]\x1b[0m ", time)
                }
                _ => "".to_owned(),
            }
        }
        None => "".to_owned(),
    };

    // Helper to check if Expr is a String Literal
    let lit_str = if let Expr::Lit(ExprLit {
        lit: Lit::Str(ref s),
        ..
    }) = msg
    {
        Some(s)
    } else {
        None
    };

    if let Some(s) = lit_str {
        // Case 1: Message is a string literal (standard println! behavior)
        let fmt_val = s.value();
        // Concatenate prefix and format string
        let fmt_with_prefix = if !prefix.is_empty() {
            format!("{}{}", prefix, fmt_val)
        } else {
            fmt_val
        };

        if let Some(indent_expr) = indent {
            // Prepend indentation placeholder
            let final_fmt_str = format!("{}{}", "{}", fmt_with_prefix);
            let final_fmt = LitStr::new(&final_fmt_str, s.span());

            quote! {
                #macro_name!(#final_fmt, " ".repeat(#indent_expr), #args)
            }
            .into()
        } else {
            let final_fmt = LitStr::new(&fmt_with_prefix, s.span());

            quote! {
                #macro_name!(#final_fmt, #args)
            }
            .into()
        }
    } else {
        // Case 2: Message is an expression (e.g. "foo".red())
        // We cannot treat it as a format string for println!, so we treat it as an argument.
        // We assume 'args' are likely empty or intended to follow the message.

        if let Some(indent_expr) = indent {
            let fmt_str = format!("{}{}{}", "{}", prefix, "{}");
            let final_fmt = LitStr::new(&fmt_str, msg.span());

            quote! {
                #macro_name!(#final_fmt, " ".repeat(#indent_expr), #msg, #args)
            }
            .into()
        } else {
            let fmt_str = format!("{}{}", prefix, "{}");
            let final_fmt = LitStr::new(&fmt_str, msg.span());

            quote! {
                #macro_name!(#final_fmt, #msg, #args)
            }
            .into()
        }
    }
}

/// [preset: Ident], [indent: LitInt], msg: Expr, args: Punctuated<Expr, Token![,]>
#[proc_macro]
pub fn log(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as LogArgs);
    generate_log(args, false)
}

/// [preset: Ident], [indent: LitInt], msg: Expr, args: Punctuated<Expr, Token![,]>
#[proc_macro]
pub fn elog(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as LogArgs);
    generate_log(args, true)
}

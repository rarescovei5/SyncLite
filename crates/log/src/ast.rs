use syn::{
    Expr, Ident, LitInt, Result, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

pub struct LogArgs {
    pub preset: Option<Ident>,
    pub indent: Option<LitInt>,
    pub msg: Expr,
    pub args: Punctuated<Expr, Token![,]>,
}

impl Parse for LogArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut preset = None;
        let mut indent = None;

        // check for preset: Ident
        if input.peek(Ident) {
            preset = Some(input.parse()?);
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        // Check for indent: Expr ( A number )
        if input.peek(LitInt) {
            indent = Some(input.parse()?);
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let msg: Expr = input.parse()?;

        let args = if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            Punctuated::parse_terminated(input)?
        } else {
            Punctuated::new()
        };

        Ok(LogArgs {
            preset,
            indent,
            msg,
            args,
        })
    }
}

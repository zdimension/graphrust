use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::parse::{Parse, Parser};
use syn::{Expr, LitStr, Token};

struct MdArgs {
    ui: Expr,
    md: LitStr,
}

impl Parse for MdArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ui: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let md: LitStr = input.parse()?;
        Ok(MdArgs { ui, md })
    }
}

#[derive(Default)]
struct State {
    should_insert_newline: bool,
    strong: bool,
    emphasis: bool,
}

#[proc_macro]
pub fn md(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let MdArgs { ui, md } = syn::parse_macro_input!(input);
    let md = md.value();
    let mut parser = pulldown_cmark::Parser::new(&md);
    let mut state = State::default();
    let mut result = TokenStream::new();
    let our_ui = Ident::new("our_ui", proc_macro2::Span::call_site());
    macro_rules! newline {
        () => {
            result.extend(quote! {
                #our_ui.label("\n");
            });
        };
    }
    use pulldown_cmark::{Event, Tag, TagEnd};
    for e in parser {
        match e {
            Event::Start(Tag::Paragraph) => {
                if state.should_insert_newline {
                    newline!();
                }
                state.should_insert_newline = true;
            }
            Event::End(TagEnd::Paragraph) => {
                state.should_insert_newline = false;
            }
            Event::Start(Tag::Strong) => {
                state.strong = true;
            }
            Event::End(TagEnd::Strong) => {
                state.strong = false;
            }
            Event::Start(Tag::Emphasis) => {
                state.emphasis = true;
            }
            Event::End(TagEnd::Emphasis) => {
                state.emphasis = false;
            }
            Event::Text(t) => {
                let t = t.as_ref();
                let mut text_buf = quote! {
                    egui::RichText::new(#t)
                };
                if state.strong {
                    text_buf = quote! {
                        #text_buf.strong()
                    };
                }
                if state.emphasis {
                    text_buf = quote! {
                        #text_buf.italics()
                    };
                }
                result.extend(quote! {
                    #our_ui.label(#text_buf);
                });
            }
            _ => unimplemented!("{:?}", e),
        }
    }
    let final_code = quote! {
        #ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 0.0), egui::Layout::left_to_right(egui::Align::BOTTOM).with_main_wrap(true), |#our_ui| {
            #our_ui.spacing_mut().item_spacing.x = 0.0;
            let height = #our_ui.text_style_height(&egui::TextStyle::Body);
            #our_ui.set_row_height(height);
            #result
        });
    };
    final_code.into()
}

use derivative::Derivative;
use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::parse::Parse;
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

#[derive(Derivative)]
#[derivative(Default)]
struct State {
    should_insert_newline: bool,
    strong: bool,
    emphasis: bool,
    #[derivative(Default(value = "-1"))]
    indentation: i64,
    list_point: Option<u64>,
}

#[proc_macro]
pub fn md(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let MdArgs { ui, md } = syn::parse_macro_input!(input);
    let md = md.value();
    let parser = pulldown_cmark::Parser::new(&md);
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

    macro_rules! width_body_space {
        () => {
            quote! {
                {
                    let id = egui::TextStyle::Body.resolve(#our_ui.style());
                    #our_ui.fonts(|f| f.glyph_width(&id, ' '))
                }
            }
        };
    }

    macro_rules! height_body {
        () => {
            quote! {
                #our_ui.text_style_height(&egui::TextStyle::Body)
            }
        };
    }

    macro_rules! bullet_point {
        () => {{
            let width_body_space = width_body_space!();
            let height_body = height_body!();
            quote! {
                {
                    let (rect, _) = #our_ui.allocate_exact_size(
                        egui::vec2(#width_body_space * 4.0, #height_body),
                        egui::Sense::hover(),
                    );
                    #our_ui.painter().circle_filled(
                        rect.center(),
                        rect.height() / 6.0,
                        #our_ui.visuals().strong_text_color(),
                    );
                    let _ = #our_ui.allocate_exact_size(
                        egui::vec2(#width_body_space, #height_body),
                        egui::Sense::hover(),
                    );
                }
            }
        }};
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
                state.should_insert_newline = true;
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
            Event::Start(Tag::List(number)) => {
                state.indentation += 1;
                state.list_point = number;
            }
            Event::End(TagEnd::List(_)) => {
                state.indentation -= 1;
                if state.indentation == -1 {
                    newline!();
                    state.should_insert_newline = true;
                }
            }
            Event::Start(Tag::Item) => {
                newline!();
                let spaces = state.indentation as usize * 4;
                result.extend(quote! {
                    #our_ui.label(" ".repeat(#spaces));
                });
                state.should_insert_newline = false;
                /*if let Some(number) = state.list_point.take() {
                    todo!();
                    /*number += 1;
                    state.list_point = Some(number);*/
                } else if state.indentation >= 1 {
                    todo!();
                } else*/
                {
                    result.extend(bullet_point!());
                }
            }
            Event::End(TagEnd::Item) => {}
            Event::Text(t) => {
                let t = t.as_ref();
                let height_body = height_body!();
                let mut text_buf = quote! {
                    egui::RichText::new(#t).line_height(Some(#height_body * 1.25))
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
            #result
        });
    };
    final_code.into()
}

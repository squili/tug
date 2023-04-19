use std::path::PathBuf;

use knuffel::span::{LineSpan, Spanned};

use super::span::ParseSpan;

#[derive(knuffel::Decode, Default, Debug)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedDocument {
    #[knuffel(children(name = "image"))]
    pub images: Vec<ParsedImage>,
    #[knuffel(children(name = "container"))]
    pub containers: Vec<ParsedContainer>,
    #[knuffel(children(name = "network"))]
    pub networks: Vec<ParsedNetwork>,
    #[knuffel(children(name = "volume"))]
    pub volumes: Vec<ParsedVolume>,
}

#[derive(knuffel::Decode, Debug)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedImage {
    #[knuffel(argument)]
    pub name: Spanned<String, ParseSpan>,
    #[knuffel(property)]
    pub reference: Spanned<String, ParseSpan>,
    #[knuffel(property, default)]
    pub local: bool,
}

#[derive(knuffel::Decode, Debug)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedContainer {
    #[knuffel(argument)]
    pub name: Spanned<String, ParseSpan>,
    #[knuffel(child, unwrap(argument))]
    pub image: Spanned<String, ParseSpan>,
    #[knuffel(child, unwrap(argument))]
    pub command: Option<Spanned<String, ParseSpan>>,
    #[knuffel(children(name = "port"))]
    pub ports: Vec<ParsedContainerPort>,
    #[knuffel(children(name = "inject"))]
    pub injects: Vec<ParsedContainerInject>,
    #[knuffel(children(name = "network"))]
    pub networks: Vec<ParsedContainerNetwork>,
    #[knuffel(children(name = "mount"))]
    pub mounts: Vec<ParsedContainerMount>,
}

#[derive(Debug)]
pub enum ParsedContainerPort {
    Shorthand(u16),
    Explicit(ParsedExplicitContainerPort),
}

impl knuffel::Decode<LineSpan> for ParsedContainerPort {
    fn decode_node(
        node: &knuffel::ast::SpannedNode<LineSpan>,
        ctx: &mut knuffel::decode::Context<LineSpan>,
    ) -> Result<Self, knuffel::errors::DecodeError<LineSpan>> {
        match node.arguments.as_slice() {
            [] => knuffel::Decode::decode_node(node, ctx).map(Self::Explicit),
            [value] => knuffel::DecodeScalar::decode(value, ctx).map(Self::Shorthand),
            [_, further, ..] => Err(knuffel::errors::DecodeError::unexpected(
                &further.literal,
                "argument",
                "shorthand port declaration only takes one argument".to_string(),
            )),
        }
    }
}

#[derive(knuffel::Decode, Debug)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedExplicitContainerPort {
    #[knuffel(property)]
    pub container: u16,
    #[knuffel(property)]
    pub host: u16,
    #[knuffel(property, default)]
    pub protocol: ParsedProtocol,
}

#[derive(knuffel::DecodeScalar, Default, Debug, Clone)]
#[knuffel(span_type = LineSpan)]
pub enum ParsedProtocol {
    #[default]
    Tcp,
    Udp,
}

impl ParsedProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParsedProtocol::Tcp => "tcp",
            ParsedProtocol::Udp => "udp",
        }
    }
}

impl ToString for ParsedProtocol {
    fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}

#[derive(knuffel::Decode, Debug, Clone)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedContainerInject {
    #[knuffel(property)]
    pub at: Spanned<PathBuf, ParseSpan>,
    #[knuffel(property)]
    pub path: PathBuf,
}

#[derive(knuffel::Decode, Debug, Clone)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedContainerNetwork {
    #[knuffel(argument)]
    pub name: Spanned<String, ParseSpan>,
    #[knuffel(children(name = "alias"), unwrap(argument))]
    pub aliases: Vec<String>,
}

#[derive(knuffel::Decode, Debug, Clone)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedContainerMount {
    #[knuffel(property(name = "type"))]
    pub kind: ParsedContainerMountType,
    #[knuffel(property)]
    pub name: Spanned<String, ParseSpan>,
    #[knuffel(property)]
    pub destination: String,
}

#[derive(knuffel::DecodeScalar, Debug, Clone)]
#[knuffel(span_type = LineSpan)]
pub enum ParsedContainerMountType {
    Volume,
}

#[derive(knuffel::Decode, Debug)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedNetwork {
    #[knuffel(argument)]
    pub name: Spanned<String, ParseSpan>,
    #[knuffel(child)]
    pub dns_enabled: bool,
    #[knuffel(child)]
    pub internal: bool,
    #[knuffel(child, unwrap(property), default = "bridge".into())]
    pub driver: String,
}

#[derive(knuffel::Decode, Debug)]
#[knuffel(span_type = LineSpan)]
pub struct ParsedVolume {
    #[knuffel(argument)]
    pub name: Spanned<String, ParseSpan>,
    #[knuffel(child, unwrap(property), default = "local".into())]
    pub driver: String,
}

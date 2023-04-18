use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_util::{ready, AsyncWrite};
use hyper::{
    body::{Bytes, Sender},
    Body,
};
use miette::IntoDiagnostic;
use serde::Serialize;

#[derive(PartialEq, Eq, Hash)]
pub enum XTug {
    Name,
    Group,
    InjectFingerprint,
}

impl AsRef<str> for XTug {
    fn as_ref(&self) -> &str {
        match self {
            XTug::Name => "X-Tug-Name",
            XTug::Group => "X-Tug-Group",
            XTug::InjectFingerprint => "X-Tug-Inject-Fingerprint",
        }
    }
}

impl ToString for XTug {
    fn to_string(&self) -> String {
        self.as_ref().to_string()
    }
}

impl Serialize for XTug {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

pub trait IntoDiagnosticShorthand<T, E> {
    fn d(self) -> Result<T, miette::Report>;
}

impl<T, E: std::error::Error + Send + Sync + 'static> IntoDiagnosticShorthand<T, E> for Result<T, E> {
    fn d(self) -> Result<T, miette::Report> {
        self.into_diagnostic()
    }
}

pub struct BodyWriter {
    sender: Sender,
}

impl BodyWriter {
    pub fn new() -> (BodyWriter, Body) {
        let (sender, body) = Body::channel();
        (BodyWriter { sender }, body)
    }
}

impl AsyncWrite for BodyWriter {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        if ready!(this.sender.poll_ready(cx)).is_err() {
            return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()));
        }
        if this.sender.try_send_data(Bytes::from(buf.to_vec())).is_err() {
            return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()));
        }
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

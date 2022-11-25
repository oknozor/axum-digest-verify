use std::fmt;
use std::future::Future;
use std::io::Error;
use std::pin::Pin;
use std::time::Duration;
use tower::{BoxError, Service};
use std::task::{Context, Poll, ready};
use http::{HeaderValue, Request, Response};
use pin_project::pin_project;
use tokio::time::Sleep;
use tower::Layer;
use http::status::StatusCode;
use http_body::Body;
use http_body_util::{BodyExt, StreamBody};
use http_body_util::combinators::BoxBody;
use sha2::digest::typenum::op;
use crate::digest::DigestPart;
use bytes::Bytes;

mod util;
mod digest;

#[derive(Debug, Clone)]
pub struct VerifyDigest<S> {
    inner: S,
}

impl<S> VerifyDigest<S> {
    fn validate<ReqBody, RespBody>(&mut self, req: &Request<ReqBody>) -> Result<(), Response<RespBody>>
        where RespBody: Default,
              ReqBody: Body<Data = Bytes> + Send + Sync + 'static,
    {
        match req.headers().get("Digest") {
            None => {
                let mut res = Response::new(RespBody::default());
                *res.status_mut() = StatusCode::NOT_ACCEPTABLE;
                Err(res)
            }
            Some(digest) => {
                let digest = DigestPart::try_from_header(digest);
                match digest {
                    None => Err(Response::new(RespBody::default())),
                    Some(digest) => {
                        println!("{:?}", digest);
                        // How do I get the request body ?
                        Ok(())
                    }
                }
            }
        }
    }
}

impl<S> VerifyDigest<S> {
    fn new(inner: S) -> VerifyDigest<S> {
        Self { inner }
    }
}

#[derive(Debug, Clone)]
pub struct VerifyDigestLayer;

impl<S> Layer<S> for VerifyDigestLayer {
    type Service = VerifyDigest<S>;

    fn layer(&self, inner: S) -> Self::Service {
        VerifyDigest::new(inner)
    }
}

impl<ReqBody, ResBody, S> Service<Request<ReqBody>> for VerifyDigest<S>
    where
        S: Service<Request<ReqBody>, Response=Response<ResBody>>,
        ResBody: Default,
        ReqBody: Body<Data = Bytes> + Send + Sync + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future, ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Have to map the error type here as well.
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let body = req.boxed();
        let bytes = to_bytes(body).call();
        match self.validate(&req) {
            Ok(_) => ResponseFuture::future(self.inner.call(req)),
            Err(res) => ResponseFuture::missing_digest_header(res),
        }
    }
}

#[pin_project]
pub struct ResponseFuture<F, B> {
    #[pin]
    kind: Kind<F, B>,
}

impl<F, B> ResponseFuture<F, B> {
    fn future(future: F) -> Self {
        Self {
            kind: Kind::Future { future },
        }
    }

    fn missing_digest_header(res: Response<B>) -> Self {
        Self {
            kind: Kind::Error {
                response: Some(res),
            },
        }
    }
}

#[pin_project(project = KindProj)]
enum Kind<F, B> {
    Future {
        #[pin]
        future: F,
    },
    Error {
        response: Option<Response<B>>,
    },
}

impl<F, E, B> Future for ResponseFuture<F, B>
    where
        F: Future<Output=Result<Response<B>, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().kind.project() {
            KindProj::Future { future } => future.poll(cx),
            KindProj::Error { response } => {
                let response = response.take().unwrap();
                Poll::Ready(Ok(response))
            }
        }
    }
}


#[derive(Debug, Default)]
pub struct VerifyDigestError(());

impl fmt::Display for VerifyDigestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("Digest verification failed")
    }
}

impl std::error::Error for VerifyDigestError {}

pub(crate) async fn to_bytes<T>(body: T) -> Result<Bytes, T::Error>
    where
        T: Body,
{
    futures_util::pin_mut!(body);

    // If there's only 1 chunk, we can just return Buf::to_bytes()
    let mut first = if let Some(buf) = body.data().await {
        buf?
    } else {
        return Ok(Bytes::new());
    };

    let second = if let Some(buf) = body.data().await {
        buf?
    } else {
        return Ok(first.copy_to_bytes(first.remaining()));
    };

    // With more than 1 buf, we gotta flatten into a Vec first.
    let cap = first.remaining() + second.remaining() + body.size_hint().lower() as usize;
    let mut vec = Vec::with_capacity(cap);
    vec.put(first);
    vec.put(second);

    while let Some(buf) = body.data().await {
        vec.put(buf?);
    }

    Ok(vec.into())
}
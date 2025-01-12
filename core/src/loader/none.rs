use super::Loader;
use contextual::{DisplayWithContext, WithContext};
use rdf_types::IriVocabulary;
use std::fmt;

use crate::future::{BoxFuture, FutureExt};
use crate::LoadingResult;

/// Dummy loader.
///
/// A dummy loader that does not load anything.
/// Can be useful when you know that you will never need to load remote resource.
///
/// Raises an `LoadingDocumentFailed` at every attempt to load a resource.
#[derive(Debug, Default)]
pub struct NoLoader;

#[derive(Debug, thiserror::Error)]
#[error("cannot load `{0}`")]
pub struct CannotLoad<I>(I);

impl<I: DisplayWithContext<N>, N> DisplayWithContext<N> for CannotLoad<I> {
	fn fmt_with(&self, vocabulary: &N, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "cannot load `{}`", self.0.with(vocabulary))
	}
}

impl<I> Loader<I> for NoLoader {
	type Error = CannotLoad<I>;

	#[inline(always)]
	fn load_with<'a, V>(
		&'a mut self,
		_vocabulary: &'a mut V,
		url: I,
	) -> BoxFuture<'a, LoadingResult<I, CannotLoad<I>>>
	where
		V: IriVocabulary<Iri = I>,
		//
		V: Send + Sync,
		I: 'a + Send,
	{
		async move { Err(CannotLoad(url)) }.boxed()
	}
}

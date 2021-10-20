use crate::{
	context::{self, Loader},
	expansion,
	compaction,
	Context,
	ContextMut,
	ContextMutProxy,
	Error,
	Id,
	Indexed,
	Object,
	util::{AsJson, JsonFrom}
};
use futures::future::{FutureExt, BoxFuture};
use generic_json::{Json, JsonClone};
use cc_traits::Len;
use iref::{Iri, IriBuf};
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};

/// Result of the document expansion algorithm.
///
/// It is just an alias for a set of (indexed) objects.
pub type ExpandedDocument<J, T> = HashSet<Indexed<Object<J, T>>>;

/// JSON-LD document.
///
/// This trait represent a JSON-LD document that can be expanded into an [`ExpandedDocument`].
/// It is notably implemented for the [`JsonValue`] type.
pub trait Document<T: Id> {
	type Json: Json;

	/// Document location, if any.
	fn base_url(&self) -> Option<Iri>;

	/// Expand the document with a custom base URL, initial context, document loader and
	/// expansion options.
	///
	/// If you do not wish to set the base URL and expansion options yourself, the
	/// [`expand`](`Document::expand`) method is more appropriate.
	///
	/// This is an asynchronous method since expanding the context may require loading remote
	/// ressources. It returns a boxed [`Future`](`std::future::Future`) to the result.
	fn expand_with<'a, C: 'a + ContextMut<T> + Send + Sync, L: 'a + Loader + Send + Sync>(
		&'a self,
		base_url: Option<Iri>,
		context: &'a C,
		loader: &'a mut L,
		options: expansion::Options,
	) -> BoxFuture<'a, Result<ExpandedDocument<Self::Json, T>, Error>>
	where
		Self::Json: expansion::JsonExpand,
		C::LocalContext: From<L::Output> + From<Self::Json>,
		L::Output: Into<Self::Json>, // TODO get rid of this bound?
		T: 'a + Send + Sync;

	/// Expand the document.
	///
	/// Uses the given initial context and the given document loader.
	/// The default implementation is equivalent to [`expand_with`](`Document::expand_with`), but
	/// uses the document [`base_url`](`Document::base_url`), with the default
	/// options.
	///
	/// This is an asynchronous method since expanding the context may require loading remote
	/// ressources. It returns a boxed [`Future`](`std::future::Future`) to the result.
	///
	/// # Example
	/// ```
	/// # fn main() -> Result<(), json_ld::Error> {
	/// use async_std::task;
	/// use json_ld::{Document, JsonContext, NoLoader};
	///
	/// let doc = json::parse("{
	///   \"@context\": {
	///     \"name\": \"http://xmlns.com/foaf/0.1/name\",
	///     \"knows\": \"http://xmlns.com/foaf/0.1/knows\"
	///   },
	///   \"@id\": \"http://timothee.haudebourg.net/\",
	///   \"name\": \"Timothée Haudebourg\",
	///   \"knows\": [
	///     {
	///       \"name\": \"Amélie Barbe\"
	///     }
	///   ]
	/// }").unwrap();
	/// let expanded_doc = task::block_on(doc.expand::<JsonContext, _>(&mut NoLoader))?;
	/// # Ok(())
	/// # }
	/// ```
	fn expand<'a, C: 'a + ContextMut<T> + Send + Sync, L: Loader + Send + Sync>(
		&'a self,
		loader: &'a mut L,
	) -> BoxFuture<'a, Result<ExpandedDocument<Self::Json, T>, Error>>
	where
		Self: Send + Sync,
		Self::Json: expansion::JsonExpand,
		C::LocalContext: From<L::Output> + From<Self::Json>,
		L::Output: Into<Self::Json>,
		T: 'a + Send + Sync,
	{
		async move {
			let context = C::new(self.base_url());
			self.expand_with(
				self.base_url(),
				&context,
				loader,
				expansion::Options::default(),
			)
			.await
		}
		.boxed()
	}

	/// Compact the document with a custom base URL, context, document loader and options.
	/// 
	/// The `meta_context` parameter is a function to convert the metadata
	/// associated to the input context (JSON representation) to `K::MetaData`.
	/// The `meta_document` parameter is another conversion function for the
	/// metadata attached to the document.
	fn compact_with<
		'a,
		K: JsonFrom<Self::Json> + JsonFrom<<C::Target as Context<T>>::LocalContext>,
		C: ContextMutProxy<T> + AsJson<<C::Target as Context<T>>::LocalContext, K>,
		L: Loader,
		M1,
		M2
	>(
		&'a self,
		base_url: Option<Iri<'a>>,
		context: &'a C,
		loader: &'a mut L,
		options: compaction::Options,
		meta_context: M1,
		meta_document: M2
	) -> BoxFuture<'a, Result<K, Error>>
	where
		Self: Sync,
		T: 'a + Send + Sync,
		Self::Json: expansion::JsonExpand + compaction::JsonSrc,
		C: Send + Sync,
		<C::Target as Context<T>>::LocalContext: compaction::JsonSrc + From<L::Output> + From<Self::Json>,
		C::Target: Send + Sync,
		L: 'a + Send + Sync,
		M1: 'a + Clone + Send + Sync + Fn(Option<&<<C::Target as Context<T>>::LocalContext as Json>::MetaData>) -> K::MetaData,
		M2: 'a + Clone + Send + Sync + Fn(Option<&<Self::Json as Json>::MetaData>) -> K::MetaData,
		L::Output: Into<Self::Json>
	{
		use compaction::Compact;
		async move {
			let json_context = context.as_json_with(meta_context);
			let context = context::Inversible::new(context.deref());
			let expanded = self
				.expand_with(base_url, &C::Target::new(base_url), loader, options.into())
				.await?;

			let compacted: K = if expanded.len() == 1 && options.compact_arrays {
				expanded
					.into_iter()
					.next()
					.unwrap()
					.compact_with(context.clone(), context.clone(), None, loader, options, meta_document.clone())
					.await?
			} else {
				expanded
					.compact_with(context.clone(), context.clone(), None, loader, options, meta_document.clone())
					.await?
			};

			let (mut map, metadata) = match compacted.into_parts() {
				(generic_json::Value::Array(items), metadata) => {
					let mut map = K::Object::default();
					if !items.is_empty() {
						use crate::syntax::{Keyword, Term};
						let key = crate::compaction::compact_iri::<Self::Json, _, _>(
							context.clone(),
							&Term::Keyword(Keyword::Graph),
							true,
							false,
							options,
						)?;
						map.insert(K::new_key(&key.unwrap(), meta_document(None)), K::array(items, metadata));
					}

					(map, meta_document(None))
				}
				(generic_json::Value::Object(map), metadata) => (map, metadata),
				_ => panic!("invalid compact document"),
			};

			if !map.is_empty() && !json_context.is_null() && !json_context.is_empty_array_or_object() {
				map.insert(K::new_key("@context", meta_document(None)), json_context);
			}

			Ok(K::object(map, metadata))
		}
		.boxed()
	}

	/// Compact the document.
	fn compact<
		'a,
		C: ContextMutProxy<T> + AsJson<Self::Json, Self::Json>,
		L: Loader
	>(
		&'a self,
		context: &'a C,
		loader: &'a mut L,
	) -> BoxFuture<'a, Result<Self::Json, Error>>
	where
		Self: Sync,
		Self::Json: JsonFrom<Self::Json> + expansion::JsonExpand + compaction::JsonSrc + From<L::Output>,
		<Self::Json as Json>::MetaData: Default,
		T: 'a + Send + Sync,
		C::Target: Context<T, LocalContext=Self::Json>,
		C: Send + Sync,
		C::Target: Send + Sync,
		L: 'a + Send + Sync,
		L::Output: Into<Self::Json>
	{
		self.compact_with(
			self.base_url(),
			context,
			loader,
			compaction::Options::default(),
			|m| m.cloned().unwrap_or_default(),
			|m| m.cloned().unwrap_or_default()
		)
	}
}

/// Default JSON document implementation.
impl<J: Json, T: Id> Document<T> for J {
	type Json = Self;

	/// Returns `None`.
	///
	/// Use [`RemoteDocument`] to attach a base URL to a `JsonValue` document.
	fn base_url(&self) -> Option<Iri> {
		None
	}

	fn expand_with<'a, C: ContextMut<T> + Send + Sync, L: Loader + Send + Sync>(
		&'a self,
		base_url: Option<Iri>,
		context: &'a C,
		loader: &'a mut L,
		options: expansion::Options,
	) -> BoxFuture<'a, Result<ExpandedDocument<Self, T>, Error>>
	where
		Self: expansion::JsonExpand,
		C::LocalContext: From<L::Output> + From<Self>,
		L::Output: Into<Self>,
		T: 'a + Send + Sync,
	{
		expansion::expand(context, self, base_url, loader, options).boxed()
	}
}

/// Remote JSON-LD document.
///
/// Represent a document located at a given base URL.
/// This is the result of loading a document with [`Loader::load`](`crate::Loader::load`).
/// It is a simple wrapper that [`Deref`] to the underlying document while remembering its
/// base URL.
///
/// # Example
/// ```
/// use static_iref::*;
///
/// use async_std::task;
/// use json::JsonValue;
/// use json_ld::{
///   Loader,
///   FsLoader,
///   RemoteDocument
/// };
///
/// // Prepare the loader.
/// let mut loader = FsLoader::new();
/// loader.mount(iri!("https://w3c.github.io/json-ld-api"), "json-ld-api");
///
/// // Load the remote document.
/// let url = iri!("https://w3c.github.io/json-ld-api/tests/expand-manifest.jsonld");
/// let doc: RemoteDocument<JsonValue> = task::block_on(loader.load(url)).unwrap();
/// ```
#[derive(Clone)]
pub struct RemoteDocument<D> {
	/// The base URL of the document.
	base_url: IriBuf,

	/// The document contents.
	doc: D,
}

impl<D> RemoteDocument<D> {
	/// Create a new remote document from the document contents and base URL.
	pub fn new(doc: D, base_url: Iri) -> RemoteDocument<D> {
		RemoteDocument {
			base_url: base_url.into(),
			doc,
		}
	}

	/// Consume the remote document and return the inner document.
	pub fn into_document(self) -> D {
		self.doc
	}

	/// Consume the remote document and return the inner document along with its base URL.
	pub fn into_parts(self) -> (D, IriBuf) {
		(self.doc, self.base_url)
	}
}

/// A Remote document is a document.
impl<T: Id, D: Document<T>> Document<T> for RemoteDocument<D> {
	type Json = D::Json;

	fn base_url(&self) -> Option<Iri> {
		Some(self.base_url.as_iri())
	}

	fn expand_with<'a, C: 'a + ContextMut<T> + Send + Sync, L: 'a + Loader + Send + Sync>(
		&'a self,
		base_url: Option<Iri>,
		context: &'a C,
		loader: &'a mut L,
		options: expansion::Options,
	) -> BoxFuture<'a, Result<ExpandedDocument<Self::Json, T>, Error>>
	where
		D::Json: expansion::JsonExpand,
		C::LocalContext: From<L::Output> + From<Self::Json>,
		L::Output: Into<Self::Json>,
		T: 'a + Send + Sync,
	{
		self.doc.expand_with(base_url, context, loader, options)
	}
}

impl<D> Deref for RemoteDocument<D> {
	type Target = D;

	fn deref(&self) -> &D {
		&self.doc
	}
}

impl<D> DerefMut for RemoteDocument<D> {
	fn deref_mut(&mut self) -> &mut D {
		&mut self.doc
	}
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompactIri(str);

impl CompactIri {
	pub unsafe fn new_unchecked(s: &str) -> &Self {
		std::mem::transmute(s)
	}

	pub fn as_str(&self) -> &str {
		&self.0
	}

	pub fn to_owned(&self) -> CompactIriBuf {
		CompactIriBuf(self.0.to_owned())
	}

	pub fn prefix(&self) -> &str {
		let i = self.find(':').unwrap();
		&self[0..i]
	}

	pub fn suffix(&self) -> &str {
		let i = self.find(':').unwrap();
		&self[i+1..]
	}
}

impl std::ops::Deref for CompactIri {
	type Target = str;

	fn deref(&self) -> &str {
		&self.0
	}
}

impl std::borrow::Borrow<str> for CompactIri {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl AsRef<str> for CompactIri {
	fn as_ref(&self) -> &str {
		&self.0
	}
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompactIriBuf(String);

impl CompactIriBuf {
	pub fn as_compact_iri(&self) -> &CompactIri {
		unsafe { CompactIri::new_unchecked(&self.0) }
	}
}

impl std::borrow::Borrow<CompactIri> for CompactIriBuf {
	fn borrow(&self) -> &CompactIri {
		self.as_compact_iri()
	}
}

impl std::ops::Deref for CompactIriBuf {
	type Target = CompactIri;

	fn deref(&self) -> &CompactIri {
		self.as_compact_iri()
	}
}
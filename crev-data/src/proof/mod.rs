//! Some common stuff for both Review and Trust Proofs

use chrono::{self, prelude::*};
use crev_common;
use failure::bail;
use std::{
    default, fmt, fs,
    io::{self, BufRead},
    mem,
    path::Path,
};

pub mod content;
pub mod package_info;
pub mod review;
pub mod revision;
pub mod trust;

pub use self::{package_info::*, revision::*, trust::*};
pub use crate::proof::content::{Content, ContentCommon};
pub use review::*;

use crate::Result;

const MAX_PROOF_BODY_LENGTH: usize = 32_000;

pub type Date = chrono::DateTime<FixedOffset>;

#[derive(Copy, Clone, Debug)]
pub enum ProofType {
    Code,
    Package,
    Trust,
}

impl ProofType {
    fn begin_block(self) -> &'static str {
        match self {
            ProofType::Code => review::Code::BEGIN_BLOCK,
            ProofType::Package => review::Package::BEGIN_BLOCK,
            ProofType::Trust => Trust::BEGIN_BLOCK,
        }
    }
    fn begin_signature(self) -> &'static str {
        match self {
            ProofType::Code => review::Code::BEGIN_SIGNATURE,
            ProofType::Package => review::Package::BEGIN_SIGNATURE,
            ProofType::Trust => Trust::BEGIN_SIGNATURE,
        }
    }
    fn end_block(self) -> &'static str {
        match self {
            ProofType::Code => review::Code::END_BLOCK,
            ProofType::Package => review::Package::END_BLOCK,
            ProofType::Trust => Trust::END_BLOCK,
        }
    }
}

/// Serialized Proof
///
/// A signed proof containing some signed `Content`
#[derive(Debug, Clone)]
pub(crate) struct Serialized {
    /// Serialized content
    pub body: String,
    /// Signature over the body
    pub signature: String,
    /// Type of the `body` (`Content`)
    pub type_: ProofType,
}

#[derive(Debug, Clone)]
/// A `Proof` with it's content parsed and ready.
pub struct Proof {
    pub body: String,
    pub signature: String,
    pub digest: Vec<u8>,
    pub content: Content,
}

impl fmt::Display for Serialized {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.type_.begin_block())?;
        f.write_str("\n")?;
        f.write_str(&self.body)?;
        f.write_str(self.type_.begin_signature())?;
        f.write_str("\n")?;
        f.write_str(&self.signature)?;
        f.write_str("\n")?;
        f.write_str(self.type_.end_block())?;
        f.write_str("\n")?;

        Ok(())
    }
}

impl fmt::Display for Proof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.content.proof_type().begin_block())?;
        f.write_str("\n")?;
        f.write_str(&self.body)?;
        f.write_str(self.content.proof_type().begin_signature())?;
        f.write_str("\n")?;
        f.write_str(&self.signature)?;
        f.write_str("\n")?;
        f.write_str(self.content.proof_type().end_block())?;
        f.write_str("\n")?;

        Ok(())
    }
}

impl Serialized {
    pub fn to_parsed(&self) -> Result<Proof> {
        Ok(Proof {
            body: self.body.clone(),
            signature: self.signature.clone(),
            digest: crev_common::blake2b256sum(&self.body.as_bytes()),
            content: match self.type_ {
                ProofType::Code => review::Code::parse(&self.body)?.into(),
                ProofType::Package => review::Package::parse(&self.body)?.into(),
                ProofType::Trust => Trust::parse(&self.body)?.into(),
            },
        })
    }

    pub fn parse(reader: impl io::Read) -> Result<Vec<Self>> {
        let reader = std::io::BufReader::new(reader);

        #[derive(PartialEq, Eq)]
        enum Stage {
            None,
            Body,
            Signature,
        }

        impl Default for Stage {
            fn default() -> Self {
                Stage::None
            }
        }

        struct State {
            stage: Stage,
            body: String,
            signature: String,
            type_: ProofType,
            proofs: Vec<Serialized>,
        }

        impl default::Default for State {
            fn default() -> Self {
                State {
                    stage: Default::default(),
                    body: Default::default(),
                    signature: Default::default(),
                    type_: ProofType::Trust, // whatever
                    proofs: vec![],
                }
            }
        }

        impl State {
            fn process_line(&mut self, line: &str) -> Result<()> {
                match self.stage {
                    Stage::None => {
                        let line = line.trim();
                        if line.is_empty() {
                        } else if line == ProofType::Code.begin_block() {
                            self.type_ = ProofType::Code;
                            self.stage = Stage::Body;
                        } else if line == ProofType::Trust.begin_block() {
                            self.type_ = ProofType::Trust;
                            self.stage = Stage::Body;
                        } else if line == ProofType::Package.begin_block() {
                            self.type_ = ProofType::Package;
                            self.stage = Stage::Body;
                        } else {
                            bail!("Parsing error when looking for start of code review proof");
                        }
                    }
                    Stage::Body => {
                        if line.trim() == self.type_.begin_signature() {
                            self.stage = Stage::Signature;
                        } else {
                            self.body += line;
                            self.body += "\n";
                        }
                        if self.body.len() > MAX_PROOF_BODY_LENGTH {
                            bail!("Proof body too long");
                        }
                    }
                    Stage::Signature => {
                        if line.trim() == self.type_.end_block() {
                            self.stage = Stage::None;
                            self.proofs.push(Serialized {
                                body: mem::replace(&mut self.body, String::new()),
                                signature: mem::replace(&mut self.signature, String::new()),
                                type_: self.type_,
                            });
                        } else {
                            self.signature += line;
                            self.signature += "\n";
                        }
                        if self.signature.len() > 2000 {
                            bail!("Signature too long");
                        }
                    }
                }
                Ok(())
            }

            fn finish(self) -> Result<Vec<Serialized>> {
                if self.stage != Stage::None {
                    bail!("Unexpected EOF while parsing");
                }
                Ok(self.proofs)
            }
        }

        let mut state: State = Default::default();

        for line in reader.lines() {
            state.process_line(&line?)?;
        }

        state.finish()
    }
}

impl Proof {
    pub fn parse_from(path: &Path) -> Result<Vec<Self>> {
        let file = fs::File::open(path)?;
        Self::parse(io::BufReader::new(file))
    }

    pub fn parse(reader: impl io::Read) -> Result<Vec<Self>> {
        let mut v = vec![];
        for serialized in Serialized::parse(reader)?.into_iter() {
            v.push(serialized.to_parsed()?)
        }
        Ok(v)
    }

    pub fn signature(&self) -> &str {
        self.signature.trim()
    }

    pub fn verify(&self) -> Result<()> {
        let pubkey = self.content.author_id();
        pubkey.verify_signature(self.body.as_bytes(), self.signature())?;

        Ok(())
    }
}

fn equals_default_digest_type(s: &str) -> bool {
    s == default_digest_type()
}

pub fn default_digest_type() -> String {
    "blake2b".into()
}

fn equals_default_revision_type(s: &str) -> bool {
    s == default_revision_type()
}

pub fn default_revision_type() -> String {
    "git".into()
}

fn equals_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == Default::default()
}

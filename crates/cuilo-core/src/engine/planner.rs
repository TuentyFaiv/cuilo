//! Planning a piece and the pieces it requires: inputs → render → patches → lock records → validation.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use super::{Engine, EngineError, PieceRequest};
use crate::model::{Digest, Input, Name, Permissions, RelPath, Stack, Values, or_list};
use crate::pack::{
	Pack, Piece, RenderRequest, Rendered, RenderedFile, RenderedPatch, RenderedStep, own_targets, render, resolve_inputs,
	target as render_target,
};
use crate::patch::{Patch, PatchError, Patched};
use crate::plan::{Draft, OnConflict, Plan, Rules, Validated};
use crate::workspace::{FileRecord, Instance, Lockfile, PackRecord, PatchRecord, instance_id};

impl Engine {
	/// Plans generating a piece, and the pieces it requires that the project lacks; nothing is written.
	pub fn plan(&self, request: &PieceRequest<'_>) -> Result<Plan<Validated>, EngineError> {
		self.plan_pieces(request, false)
	}

	/// Plans taking over existing code: files the project already has are recorded as generated, unchanged,
	/// so later updates merge into them; missing files are created.
	pub fn plan_adopt(&self, request: &PieceRequest<'_>) -> Result<Plan<Validated>, EngineError> {
		self.plan_pieces(request, true)
	}

	fn plan_pieces(&self, request: &PieceRequest<'_>, adopt: bool) -> Result<Plan<Validated>, EngineError> {
		let pack = request.pack;
		let manifest = pack.manifest();
		let lock = self.lockfile()?;
		check_pinned(pack, &lock)?;

		let main = pack.piece(request.piece).ok_or_else(|| EngineError::UnknownPiece {
			pack: manifest.name.to_string(),
			piece: request.piece.to_owned(),
		})?;
		let mut pieces = Vec::new();
		collect_required(pack, main, &lock, &mut pieces);
		pieces.push(main);

		let stack = self.project()?.stack;
		let verb = if adopt { "adopt" } else { "add" };
		let mut plan = Plan::new(summary(verb, manifest.name.as_str(), &pieces));
		let mut added = Added {
			adopt,
			wire: request.wire && !adopt,
			..Added::default()
		};
		for piece in &pieces {
			let is_main = std::ptr::eq(*piece, main);
			let name = piece.manifest().name.as_str();
			let given = if is_main {
				request.inputs.clone()
			} else {
				shared_inputs(piece, request.inputs)
			};
			let values = self.values(name, piece, Some(&given), &stack)?;
			let variant = request
				.variant
				.filter(|variant| is_main || piece.manifest().variants.iter().any(|v| v.name.as_str() == *variant));
			let target = if is_main { request.target } else { None };
			self.add_piece(&mut plan, &mut added, pack, piece, &values, variant, target, &stack)?;
		}
		self.add_patches(&mut plan, &mut added)?;
		for instance in added.instances {
			plan.record(instance);
		}
		finish(
			plan,
			&self.workspace,
			pack,
			request.granted,
			request.force,
			OnConflict::Refuse,
		)
	}

	/// Renders a new `piece` into `plan`; its patches and steps wait in `added` until every piece is rendered.
	#[allow(clippy::too_many_arguments)]
	pub(super) fn add_piece(
		&self,
		plan: &mut Plan<Draft>,
		added: &mut Added,
		pack: &Pack,
		piece: &Piece,
		values: &Values,
		variant: Option<&str>,
		target: Option<&RelPath>,
		stack: &Stack,
	) -> Result<(), EngineError> {
		let mut request = RenderRequest {
			piece,
			values,
			stack,
			variant,
			target: target.or_else(|| self.config.paths.get(piece.manifest().name.as_str())),
		};
		let found = if request.target.is_none() {
			self.found_target(&request)?
		} else {
			None
		};
		if found.is_some() {
			request.target = found.as_ref();
		}
		let stack = &self.piece_stack(stack, render_target(&request)?.as_ref());
		let rendered = render(&RenderRequest { stack, ..request })?;
		let Rendered {
			variant,
			target,
			mut files,
			patches,
			dependencies,
			steps,
			at_default_target,
		} = rendered;
		self.adapt_files(stack, &mut files, Some(plan));
		self.format_files(stack, &mut files, Some(plan));
		for dependency in dependencies {
			plan.depend(dependency);
		}
		let mut instance = instance(pack, piece, variant, target, stack.clone(), values.clone());
		instance.files = if added.adopt {
			self.adopt_files(plan, files)?
		} else {
			add_files(plan, files)?
		};
		let aimed = self.aim(&patches, &[])?;
		if !aimed.missing.is_empty() {
			return Err(EngineError::PatchTarget(aimed.missing));
		}
		let index = added.instances.len();
		added
			.patches
			.extend(aimed.patches.into_iter().map(|patch| (index, patch)));
		if !added.adopt {
			added.steps.extend(steps.into_iter().map(|step| PendingStep {
				instance: index,
				step,
				at_default_target,
			}));
		}
		added.instances.push(instance);
		Ok(())
	}

	/// The first of the piece's own folders for `request` that the project has, when it lists alternatives;
	/// `None` leaves the choice to the render, which takes the first.
	fn found_target(&self, request: &RenderRequest<'_>) -> Result<Option<RelPath>, EngineError> {
		let alternatives = own_targets(request)?;
		if alternatives.len() < 2 {
			return Ok(None);
		}
		for folder in alternatives.into_iter().flatten() {
			if self.workspace.is_dir(&folder)? {
				return Ok(Some(folder));
			}
		}
		Ok(None)
	}

	pub(super) fn values(
		&self,
		name: &str,
		piece: &Piece,
		given: Option<&Values>,
		stack: &Stack,
	) -> Result<Values, EngineError> {
		let mut values = self.config.defaults.get(name).cloned().unwrap_or_default();
		values.extend(
			given
				.into_iter()
				.flatten()
				.map(|(name, value)| (name.clone(), value.clone())),
		);
		Ok(resolve_inputs(&piece.manifest().inputs, &values, stack)?)
	}

	/// Applies every patch, grouped by file, then the patches of each wiring step all or none when `added.wire`,
	/// recording the regions on their instances; the other steps become next steps.
	pub(super) fn add_patches(&self, plan: &mut Plan<Draft>, added: &mut Added) -> Result<(), EngineError> {
		let patches = std::mem::take(&mut added.patches);
		let mut by_file: BTreeMap<&RelPath, Vec<(usize, &Patch)>> = BTreeMap::new();
		for (index, patch) in &patches {
			by_file.entry(patch.file()).or_default().push((*index, patch));
		}
		let mut texts = Texts::new();
		for (file, file_patches) in by_file {
			let before = self
				.workspace
				.read(file)?
				.ok_or_else(|| EngineError::PatchTarget(vec![file.clone()]))?;
			let mut edited = Edited::new(before);
			for (index, patch) in file_patches {
				let mut patch = patch.clone();
				self.adapt_patch(&added.instances[index].stack, &mut patch, plan);
				let record = self.patch(plan, &mut edited.text, &patch)?;
				added.instances[index].patches.push(record);
			}
			texts.insert(file.clone(), edited);
		}
		for pending in std::mem::take(&mut added.steps) {
			let PendingStep {
				instance: index,
				step,
				at_default_target,
			} = pending;
			let Some(first) = step.patches.first().filter(|_| added.wire) else {
				plan.next_step(step.text);
				continue;
			};
			if !at_default_target {
				plan.note(
					first.preferred_file().clone(),
					"the piece is not in its default folder, so cuilo did not wire it here; wire it by hand",
				);
				plan.next_step(step.text);
				continue;
			}
			let mut aimed = self.aim(&step.patches, &[])?;
			for patch in &mut aimed.patches {
				self.adapt_patch(&added.instances[index].stack, patch, plan);
			}
			match self.wire(&mut texts, &aimed)? {
				Wiring::Applied(regions) => {
					let instance = &mut added.instances[index];
					for (record, region) in regions {
						instance.patches.push(record);
						plan.keep(region.into_bytes());
					}
					instance.wired = true;
					let mut files: Vec<RelPath> = aimed.patches.iter().map(|patch| patch.file().clone()).collect();
					files.sort();
					files.dedup();
					plan.wired(&instance.id, files, step.text);
				}
				Wiring::Skipped { file, reason } => {
					plan.note(file, format!("{reason}; wire it by hand"));
					plan.next_step(step.text);
				}
			}
		}
		for (file, edited) in texts {
			plan.modify(file, edited.before, edited.text.into_bytes());
		}
		Ok(())
	}

	/// Applies a wiring step's patches to `texts` only if the project has a file for each and every one applies.
	pub(super) fn wire(&self, texts: &mut Texts, step: &Aimed) -> Result<Wiring, EngineError> {
		if let Some(file) = step.missing.first() {
			return Ok(Wiring::skipped(file, &absent(&step.missing)));
		}
		let mut trial = Texts::new();
		let mut regions = Vec::with_capacity(step.patches.len());
		for patch in &step.patches {
			let file = patch.file();
			let edited = match trial.entry(file.clone()) {
				Entry::Occupied(entry) => entry.into_mut(),
				Entry::Vacant(entry) => match texts.get(file) {
					Some(edited) => entry.insert(edited.clone()),
					None => match self.workspace.read(file)? {
						Some(before) => entry.insert(Edited::new(before)),
						None => return Ok(Wiring::skipped(file, "the file does not exist")),
					},
				},
			};
			match self.region(&mut edited.text, patch) {
				Ok(region) => regions.push((record(patch, &region), region)),
				Err(error) => return Ok(Wiring::skipped(file, &unwired(&error))),
			}
		}
		texts.extend(trial);
		Ok(Wiring::Applied(regions))
	}

	/// Picks the file each of `patches` edits with [`Self::target`], once for each list of alternatives, so
	/// patches that list the same ones edit the same file.
	pub(super) fn aim(&self, patches: &[RenderedPatch], records: &[PatchRecord]) -> Result<Aimed, EngineError> {
		let mut picked: Vec<(&[RelPath], Option<&RelPath>)> = Vec::new();
		let mut aimed = Aimed::default();
		for patch in patches {
			let files = patch.files();
			let target = if let Some((_, target)) = picked.iter().find(|(listed, _)| *listed == files) {
				*target
			} else {
				let target = self.target(files, records)?;
				picked.push((files, target));
				target
			};
			if target.is_none() && aimed.missing.is_empty() {
				aimed.missing = files.to_vec();
			}
			aimed
				.patches
				.push(patch.at(target.unwrap_or_else(|| patch.preferred_file())));
		}
		Ok(aimed)
	}

	/// The alternative of `files` to edit: the first one an instance's patch `records` name, so an update keeps
	/// editing the file it was patched in, or else the first one the project has.
	fn target<'a>(&self, files: &'a [RelPath], records: &[PatchRecord]) -> Result<Option<&'a RelPath>, EngineError> {
		if let Some(file) = files
			.iter()
			.find(|file| records.iter().any(|record| record.file == **file))
		{
			return Ok(Some(file));
		}
		for file in files {
			if self.workspace.occupied(file)? {
				return Ok(Some(file));
			}
		}
		Ok(None)
	}

	/// Records the rendered files the project already has, keeping their content; creates the rest.
	fn adopt_files(&self, plan: &mut Plan<Draft>, files: Vec<RenderedFile>) -> Result<Vec<FileRecord>, EngineError> {
		let mut records = Vec::with_capacity(files.len());
		let mut missing = Vec::new();
		for file in files {
			match self.workspace.read(&file.path)? {
				Some(current) => {
					if current != file.contents {
						plan.note(
							file.path.clone(),
							"adopted with your differences; updates will merge into them",
						);
					}
					records.push(FileRecord {
						path: file.path,
						base: Digest::of(&file.contents),
					});
					plan.keep(file.contents);
				}
				None => missing.push(file),
			}
		}
		records.extend(add_files(plan, missing)?);
		Ok(records)
	}

	/// Applies `patch` to `text` and returns the record of the region it owns.
	pub(super) fn patch(
		&self,
		plan: &mut Plan<Draft>,
		text: &mut String,
		patch: &Patch,
	) -> Result<PatchRecord, EngineError> {
		let inserted = self.region(text, patch).map_err(|source| EngineError::Patch {
			file: patch.file().clone(),
			source,
		})?;
		let record = record(patch, &inserted);
		plan.keep(inserted.into_bytes());
		Ok(record)
	}

	/// Applies `patch` to `text` and returns the region it now owns.
	fn region(&self, text: &mut String, patch: &Patch) -> Result<String, PatchError> {
		Ok(match self.patchers.apply(text, patch)? {
			Patched::Changed { text: next, inserted } => {
				*text = next;
				inserted
			}
			Patched::AlreadyApplied { inserted } => inserted,
		})
	}
}

/// Project files being patched, by path.
pub(super) type Texts = BTreeMap<RelPath, Edited>;

/// Patches with the file each edits picked among its alternatives.
#[derive(Debug, Default)]
pub(super) struct Aimed {
	/// Each patch, editing the file picked for it, or its preferred file when the project has none of them.
	pub patches: Vec<Patch>,
	/// The alternatives of the first patch the project has none of; empty when it has a file for each.
	pub missing: Vec<RelPath>,
}

/// A file's content before the plan and its patched text so far.
#[derive(Clone, Debug)]
pub(super) struct Edited {
	/// Content on disk.
	pub before: Vec<u8>,
	/// Text with the patches applied so far.
	pub text: String,
}

impl Edited {
	/// A file not patched yet.
	pub fn new(before: Vec<u8>) -> Self {
		let text = String::from_utf8_lossy(&before).into_owned();
		Self { before, text }
	}
}

/// What became of a wiring step's patches.
#[derive(Debug)]
pub(super) enum Wiring {
	/// All applied: each region's record and text.
	Applied(Vec<(PatchRecord, String)>),
	/// None applied, because of `file`.
	Skipped {
		/// The file a patch could not edit.
		file: RelPath,
		/// Why, in plain words.
		reason: String,
	},
}

impl Wiring {
	fn skipped(file: &RelPath, reason: &str) -> Self {
		Self::Skipped {
			file: file.clone(),
			reason: reason.to_owned(),
		}
	}
}

/// Why a patch has none of its `files` to edit, in plain words.
fn absent(files: &[RelPath]) -> String {
	match files {
		[_] => "the file does not exist".to_owned(),
		several => format!("none of {} exists", or_list(several)),
	}
}

/// Why a wiring patch could not be applied, in plain words.
fn unwired(error: &PatchError) -> String {
	match error {
		PatchError::MissingAnchor(anchors) => format!("no line contains {anchors}"),
		PatchError::AmbiguousAnchor { anchor, count } => {
			format!("{count} lines contain `{anchor}`, so the place to wire is unclear")
		}
		other => other.to_string(),
	}
}

fn record(patch: &Patch, region: &str) -> PatchRecord {
	PatchRecord {
		file: patch.file().clone(),
		inserted: Digest::of(region.as_bytes()),
	}
}

/// New instances and their patches and steps, applied once every piece is rendered.
#[derive(Debug, Default)]
pub(super) struct Added {
	/// Take over existing files instead of creating them; steps are left out.
	pub adopt: bool,
	/// Apply the patches of wiring steps.
	pub wire: bool,
	/// New instances.
	pub instances: Vec<Instance>,
	/// Each patch with the index of its instance.
	pub patches: Vec<(usize, Patch)>,
	/// Each selected step, in the order the pieces were rendered.
	pub steps: Vec<PendingStep>,
}

/// A selected step waiting until every piece is rendered.
#[derive(Debug)]
pub(super) struct PendingStep {
	/// Index of its instance.
	pub instance: usize,
	/// The step.
	pub step: RenderedStep,
	/// Whether its piece is in its default folder, which its wiring patches assume.
	pub at_default_target: bool,
}

/// Refuses a pack whose files differ from those the lockfile pinned for the same version, or that is
/// not signed by the key that signed the pinned version.
pub(super) fn check_pinned(pack: &Pack, lock: &Lockfile) -> Result<(), EngineError> {
	let manifest = pack.manifest();
	match lock.pack(&manifest.name) {
		Some(record) if record.version == manifest.version && record.digest != pack.digest() => {
			Err(EngineError::PackChanged {
				name: manifest.name.clone(),
				version: manifest.version.clone(),
			})
		}
		Some(PackRecord {
			signer: Some(expected), ..
		}) if pack.signer() != Some(expected.as_str()) => Err(EngineError::SignerChanged {
			name: manifest.name.clone(),
			expected: expected.clone(),
			actual: pack.signer().map(str::to_owned),
		}),
		_ => Ok(()),
	}
}

/// Records `pack` and validates `plan` against the permissions it declares and the user granted.
pub(super) fn finish(
	mut plan: Plan<Draft>,
	workspace: &crate::workspace::Workspace,
	pack: &Pack,
	granted: Option<&Permissions>,
	force: bool,
	on_conflict: OnConflict,
) -> Result<Plan<Validated>, EngineError> {
	let manifest = pack.manifest();
	plan.record_pack(PackRecord {
		name: manifest.name.clone(),
		version: manifest.version.clone(),
		source: pack.source().map(str::to_owned),
		digest: pack.digest(),
		signer: pack.signer().map(str::to_owned),
	});
	let declared = &manifest.permissions;
	let rules = Rules {
		declared,
		granted: granted.unwrap_or(declared),
		force,
		on_conflict,
	};
	Ok(plan.validate(workspace, &rules)?)
}

/// The inputs given to the main piece that a required `piece` also declares, such as `tests` or a
/// module path; inputs that name the main piece's own thing, like `name`, are not passed on.
fn shared_inputs(piece: &Piece, given: &Values) -> Values {
	given
		.iter()
		.filter(|(name, _)| match piece.manifest().inputs.get(*name) {
			Some(Input::String { example: Some(_), .. }) | None => false,
			Some(_) => true,
		})
		.map(|(name, value)| (name.clone(), value.clone()))
		.collect()
}

/// Adds the pieces `piece` requires, deepest first, skipping those the project or the list already has.
pub(super) fn collect_required<'a>(pack: &'a Pack, piece: &'a Piece, lock: &Lockfile, pieces: &mut Vec<&'a Piece>) {
	for name in &piece.manifest().requires {
		let Some(required) = pack.piece(name.as_str()) else {
			continue;
		};
		let listed = pieces.iter().any(|listed| listed.manifest().name == *name);
		if !listed && !lock.has_piece(&pack.manifest().name, name) {
			collect_required(pack, required, lock, pieces);
			if !pieces.iter().any(|listed| listed.manifest().name == *name) {
				pieces.push(required);
			}
		}
	}
}

fn summary(verb: &str, pack: &str, pieces: &[&Piece]) -> String {
	let names: Vec<&str> = pieces.iter().map(|piece| piece.manifest().name.as_str()).collect();
	match names.split_last() {
		Some((main, [])) => format!("{verb} {pack}/{main}"),
		Some((main, required)) => format!("{verb} {pack}/{main} (with {})", required.join(", ")),
		None => format!("{verb} {pack}"),
	}
}

fn add_files(plan: &mut Plan<Draft>, files: Vec<RenderedFile>) -> Result<Vec<FileRecord>, EngineError> {
	let mut records = Vec::with_capacity(files.len());
	for file in files {
		if plan.writes(&file.path) {
			return Err(EngineError::Overlap(file.path));
		}
		records.push(FileRecord {
			path: file.path.clone(),
			base: Digest::of(&file.contents),
		});
		plan.create(file.path, file.contents);
	}
	Ok(records)
}

fn instance(
	pack: &Pack,
	piece: &Piece,
	variant: Option<Name>,
	target: Option<RelPath>,
	stack: Stack,
	values: Values,
) -> Instance {
	let manifest = pack.manifest();
	Instance {
		id: instance_id(&manifest.name, &piece.manifest().name, &values),
		pack: manifest.name.clone(),
		pack_version: manifest.version.clone(),
		piece: piece.manifest().name.clone(),
		variant,
		target,
		wired: false,
		stack,
		inputs: values,
		files: Vec::new(),
		patches: Vec::new(),
	}
}

---

title: Update generated code
description: Move pieces to a new pack version with cuilo update, which merges the pack's changes into the files you edited.
---

`cuilo update <pack>` moves every piece your project has from a pack to another version of that pack. It builds one plan for all of them. You review the plan, and applying it writes one transaction that [`cuilo undo`](/reference/cli/undo/) can revert.

## How the merge works

When cuilo writes a file, `cuilo.lock` records the SHA-256 of what it wrote as the file's `base`, and `.cuilo/objects/` keeps a copy of that content. On update, cuilo renders the new version with the inputs, variant and stack the lockfile recorded. Then it compares three versions of each file: the original cuilo wrote, the file as it is now, and the new version.

| The file since generation       | What the update does                                                     |
| ------------------------------- | ------------------------------------------------------------------------ |
| Unedited                        | Takes the new version, or deletes the file if the new version dropped it |
| Edited on other lines           | Merges both, line by line                                                |
| Edited on the same lines        | Conflict: see [Conflicts](#conflicts)                                    |
| Edited, and dropped by the pack | Stays as you left it and is no longer tracked                            |
| Deleted by you                  | Stays deleted                                                            |

The update also creates files new in this version and adds pieces it newly requires. Inputs carry over, and new inputs take their defaults. Text a patch inserted into your own files is replaced, unless you edited it. That includes the wiring of a piece you added with `--wire`. A piece you added without wiring never gets it from an update. When a wiring patch no longer fits, for example because you changed its anchor line, the update leaves that file as it is and notes why. A piece the new version dropped keeps its files. The plan notes the dropped pieces and the patches it left alone.

If `.cuilo/` has no copy of an original, for example in a fresh clone (the folder is not committed), cuilo rebuilds it by rendering the old version from the source in the lockfile. It uses the rebuilt file only if its hash matches `base`. When neither works, an edited file cannot be merged:

```text
The plan was rejected:
  src/cards/profile-card.tsx: you edited this file and its original is not available to merge with; choose markers, ours or theirs
```

## Run an update

Pass the pack name and, with `--from`, the source of the new version. Here a local pack folder moved from 1.0.0 to 1.1.0, and you had added an import to the generated file:

```sh
cuilo update cards --from ./packs/cards --dry-run
```

```text
update cards 1.0.0 → 1.1.0
  modify  src/cards/profile-card.tsx
  modify  src/app.tsx
Packages to add (offered after applying; --install adds them without asking):
  npm      clsx ^2.1
Notes:
  src/cards/profile-card.tsx: the update changes this file, so its migrations did not run here
  cuilo.lock: migration to 1.1.0: `legacyTheme` is now `theme` (1 file(s) changed)
```

`--dry-run` stops after the plan, and `--diff` adds the diff of each changed file. Run the command again without `--dry-run` to apply it: cuilo asks first, and `--yes` skips the question. Your import stays, and the new prop from the pack is merged in.

The source must hold the same pack. A source with another pack is refused, and so is a pack whose files changed without a new version:

```text
error: pack `cards` 1.0.0 differs from the version in the lockfile; it was changed without a new version
```

Without `--from`, cuilo uses the newest version it has loaded. For the official packs (`ui`, `backend`), that is the version built into your cuilo, so upgrading cuilo brings their next version. For other packs, pass `--from` with the new tag, URL or folder. After the update, the lockfile records that source. See [Pack sources](/packs/sources/).

The plan lists the packages the new version adds, or all of them when the old version cannot be loaded to compare. See [Packages a piece needs](/guides/dependencies/).

## Conflicts

A conflict is a region where you and the new version changed the same lines, or lines next to each other. By default the update refuses the whole plan and exits with code `3`:

```text
The plan was rejected:
  src/cards/profile-card.tsx: 1 region(s) conflict with your edits; choose markers, ours or theirs to resolve them
```

`--on-conflict` picks what to do instead:

| Value     | What happens in each conflicting region             |
| --------- | --------------------------------------------------- |
| `refuse`  | The plan is rejected (the default)                  |
| `markers` | Both versions are written between Git-style markers |
| `ours`    | Your lines are kept                                 |
| `theirs`  | The new version's lines are taken                   |

Changes outside the conflicting regions merge in every mode. With `markers`, the plan shows `1 conflict(s)` next to the file, and the file gets the three versions:

```sh
cuilo update cards --from ./packs/cards --on-conflict markers
```

```text
<<<<<<< yours
			<h2 className="title">{title}</h2>
||||||| original
			<h2>{title}</h2>
=======
			<h2>{title}</h2>
			{subtitle && <p>{subtitle}</p>}
>>>>>>> cards 1.1.0
```

Edit the file to keep what you want and remove the markers.

When the original is not available, `ours` keeps your whole file, `theirs` takes the whole new file, and `markers` puts both whole versions between markers. For a binary file that both sides changed, only `ours` or `theirs` work.

## Review hunk by hunk

`--review` asks about every hunk of every change before the plan is shown, like `git add -p`:

```text
modify src/cards/profile-card.tsx
@@ -3,6 +3,7 @@
 
 export interface ProfileCardProps {
 	title: string;
+	subtitle?: string;
 }
 
 export function ProfileCard({ title }: ProfileCardProps) {
Keep this hunk? [y]es, [n]o, [a]ll in file, [d]rop rest of file, [q]uit:
```

`a` keeps the rest of the file's hunks, `d` skips them, and `q` skips everything left in the plan. Then cuilo shows the narrowed plan and asks `Apply these changes? [y/N]`.

A skipped hunk counts as your edit, so later updates keep it skipped. A skipped new file is left out of the lockfile, so the next update offers it again. `--review` needs a terminal. With the default `refuse`, a plan with conflicts is rejected before the review starts, so pick another `--on-conflict` value first.

## Migrations

A pack can ship migrations for code it does not own, such as renaming a prop everywhere it is used. A migration runs when the update crosses its version, and its edits appear in the plan like any other change. Files the update itself changes are skipped, with a note.

`rename` migrations are built in. `ast_grep` migrations need the `ast-grep` program, and without it the plan is rejected:

```text
The plan was rejected:
  cuilo.lock: this migration needs `ast-grep`; install it, set `ast_grep` in your user config, or update without migrations
```

`--no-migrate` updates without running them. The lockfile then records the new version, so a later update does not run them either. To write migrations, see [Write a pack](/packs/write-a-pack/).

## Check the result

After applying, [`cuilo status`](/reference/cli/status/) shows a merged file as `modified`. Its base is now the new version, and your edits are still in it:

```text
modified  src/cards/profile-card.tsx
```

[`cuilo why`](/reference/cli/why/) tells you who wrote a line:

```text
src/cards/profile-card.tsx:2 is yours: added or changed since generation; cards 1.1.0 piece `card`
src/cards/profile-card.tsx:6 is from the pack (line 5 of the original); cards 1.1.0 piece `card`
```

`cuilo undo` restores the previous version of every file and the lockfile. To run the project's checks before applying, add `--verify`. See [Verify a plan](/guides/verify/). For every flag, see the [`cuilo update` reference](/reference/cli/update/).

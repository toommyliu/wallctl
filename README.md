# wallctl

`wallctl` is a macOS wallpaper profile controller. It lets you save the current
macOS wallpaper state as a reusable profile, group profiles into collections,
and optionally switch profiles on a fixed hourly schedule through a per-user
LaunchAgent.

This project is macOS-first. The important workflow is:

1. Configure a wallpaper in macOS System Settings.
2. Capture that current state into `wallctl`.
3. Repeat for any other profiles you want.
4. Use a collection manually or activate its schedule.

## Install

From this checkout:

```bash
./scripts/install.sh
```

That runs:

```bash
cargo install --path . --locked
```

For development without installing:

```bash
cargo run -- <command>
```

or after building:

```bash
cargo build
target/debug/wallctl <command>
```

## Interactive Selection

Run `wallctl` with no command to open an interactive menu:

```bash
wallctl
```

The menu can list collections, inspect or activate one, apply a profile once,
capture the current wallpaper, create a collection, create a light/dark dynamic
HEIC, show status, or print logs. Each row includes the matching command and a
short description.

Example:

```text
? What do you want to do?
> wallctl use        Activate a collection strategy
  wallctl inspect    Show collection metadata and validation details
  wallctl apply      Apply one profile without changing active state
```

The original command help is still available directly:

```bash
wallctl --help
wallctl help
wallctl help use
```

You do not have to type collection slugs for the common commands. If you omit
the collection argument in an interactive terminal, `wallctl` opens a picker:

```bash
wallctl use
```

Example:

```text
? Select collection
> Tahoe Aerial (tahoe-aerial) [schedule]
  Raycast Glaze 2 (raycast-glaze-2) [static]
```

Use arrow keys to move, type to filter, and press Enter to choose.

This works for `use`, `inspect`, `apply`, `capture`, and `remove`. In scripts
or non-interactive shells, keep passing the exact slug.

## Where Data Lives

`wallctl` stores its own data here:

```text
~/Library/Application Support/wallctl/
  state.toml
  collections/
  logs/wallctl.log
```

Scheduler logs live here:

```text
~/Library/Logs/wallctl/
  scheduler.out.log
  scheduler.err.log
```

Captured profiles are copied from macOS's wallpaper store:

```text
~/Library/Application Support/com.apple.wallpaper/Store/Index.plist
```

## Concepts

A collection is the thing you activate. Each collection has one strategy:

- `static`: one captured profile.
- `dynamic`: one captured profile that macOS itself changes, such as a dynamic
  HEIC wallpaper.
- `schedule`: several captured profiles selected by fixed hour-of-day slots.

A profile is a captured snapshot of the current macOS wallpaper plist. For
normal image and HEIC references, `wallctl` copies the referenced asset into the
collection and rewrites the stored profile to point at that managed copy. For
Apple Aerial wallpapers, `wallctl` stores a backup of the matching `.mov` when
possible and restores it before applying the profile if Apple's cache is
missing it.

Collection names become slugs. For example:

```bash
wallctl new static "Focus Mode"
```

creates a collection named:

```text
focus-mode
```

Use exact slugs for later commands.

## Static Wallpaper Workflow

Use this when you want one saved wallpaper setup that you can reapply later.

1. Set the wallpaper you want in macOS System Settings.
2. Create a collection:

```bash
wallctl new static "Focus Mode"
```

3. Capture the current macOS wallpaper state:

```bash
wallctl capture focus-mode
```

For static collections, the profile defaults to `default`.

4. Inspect it:

```bash
wallctl inspect focus-mode
```

5. Apply it once without making it the active collection:

```bash
wallctl apply focus-mode
```

6. Make it the active collection:

```bash
wallctl use focus-mode
```

`use` removes the `wallctl` scheduler LaunchAgent if one is installed, applies
the default profile, and records `focus-mode` as active.

## Dynamic Wallpaper Workflow

Use this for a macOS-native dynamic wallpaper, such as a dynamic HEIC where
macOS handles light/dark switching.

If you already have a dynamic HEIC, set it in macOS System Settings and skip to
collection capture below.

If you have separate light and dark images, create a dynamic HEIC first:

```bash
wallctl heic \
  --light ~/Pictures/wallpaper-light.png \
  --dark ~/Pictures/wallpaper-dark.png \
  --output ~/Pictures/wallpaper-dynamic.heic
```

The light and dark source images must have the same pixel dimensions. The
generated HEIC contains two images and Apple appearance metadata, so macOS can
switch between them when the system appearance changes.

Then:

1. Set the generated dynamic HEIC as the wallpaper in macOS System Settings.
2. Create a dynamic collection:

```bash
wallctl new dynamic "Day Night"
```

3. Capture it:

```bash
wallctl capture day-night
```

4. Activate it:

```bash
wallctl use day-night
```

Dynamic collections behave like static collections from `wallctl`'s point of
view. macOS owns the dynamic switching after the profile is applied.

## Scheduled Wallpaper Workflow

Use this when you want fixed hourly slots, such as morning, day, evening, and
night.

Create a four-slot schedule:

```bash
wallctl new schedule "Aerial Day" --preset four
```

The `four` preset creates these slots:

```text
06 morning
10 day
17 evening
20 night
```

The `three` preset creates:

```text
06 morning
10 day
19 night
```

Preset slots are invalid until you capture each profile.

For each slot:

1. Set the desired wallpaper in macOS System Settings.
2. Capture it using the exact profile name from the preset:

```bash
wallctl capture aerial-day morning
wallctl capture aerial-day day
wallctl capture aerial-day evening
wallctl capture aerial-day night
```

You can inspect progress at any time:

```bash
wallctl inspect aerial-day
```

Activation fails until every scheduled slot has a captured, valid profile.

When all profiles are ready, activate the schedule:

```bash
wallctl use aerial-day
```

For scheduled collections, `use` validates/restores required assets, writes and
loads this LaunchAgent:

```text
~/Library/LaunchAgents/local.wallctl.scheduler.plist
```

then immediately dispatches the profile for the current hour and records the
collection as active.

## Command Reference

List collections:

```bash
wallctl list
```

Show active collection and whether the live wallpaper still matches it:

```bash
wallctl status
```

Inspect a collection:

```bash
wallctl inspect
wallctl inspect <collection-slug>
```

Create collections:

```bash
wallctl new static <name>
wallctl new dynamic <name>
wallctl new schedule <name> --preset three
wallctl new schedule <name> --preset four
```

Create a light/dark dynamic HEIC:

```bash
wallctl heic --light <light-image> --dark <dark-image> --output <output.heic>
wallctl heic --light <light-image> --dark <dark-image> --output <output.heic> --force
```

Capture the current macOS wallpaper profile:

```bash
wallctl capture
wallctl capture <collection-slug>
wallctl capture <collection-slug> <profile-name>
```

Apply a profile once without changing active state or scheduler state:

```bash
wallctl apply
wallctl apply <collection-slug>
wallctl apply <collection-slug> <profile-name>
wallctl apply --force <collection-slug> <profile-name>
```

Activate a collection:

```bash
wallctl use
wallctl use <collection-slug>
```

Run the active schedule's current slot manually:

```bash
wallctl dispatch
wallctl dispatch --force
```

Show logs:

```bash
wallctl logs
```

Remove an inactive collection:

```bash
wallctl remove
wallctl remove <collection-slug>
```

Active collections cannot be removed. Activate another collection first.

## What Touches macOS State

These commands can write the live macOS wallpaper store:

- `wallctl apply`
- `wallctl use`
- `wallctl dispatch`

These commands can modify the `wallctl` LaunchAgent:

- `wallctl use <static-or-dynamic-collection>` unloads/removes it.
- `wallctl use <scheduled-collection>` writes and loads it.

These commands only read or write `wallctl`'s own stored data:

- `wallctl new`
- `wallctl capture`
- `wallctl list`
- `wallctl inspect`
- `wallctl status`
- `wallctl logs`
- `wallctl remove`

`capture` reads the live macOS wallpaper profile, but writes only into
`wallctl`'s collection storage.

## Troubleshooting

If `wallctl status` says the live wallpaper has drifted, the macOS wallpaper
store no longer matches the active profile. Reapply the active collection:

```bash
wallctl use <collection-slug>
```

If a scheduled collection will not activate, inspect it:

```bash
wallctl inspect <collection-slug>
```

Common causes:

- A scheduled profile has not been captured yet.
- A profile is missing a wallpaper provider.
- A copied image asset was removed from the collection.
- An Aerial profile is missing its `assetID`.
- An Aerial `.mov` is missing from both Apple's cache and the collection backup.

If a command says to use an exact slug, run:

```bash
wallctl list
```

and copy the slug from the first column.

If the installed command is not found after `./scripts/install.sh`, add Cargo's
bin directory to your shell path:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

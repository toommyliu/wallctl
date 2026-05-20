# wallctl

`wallctl` is a macOS wallpaper profile controller. It captures the current
wallpaper state as a reusable profile, groups profiles into collections, and can
switch collections manually or on a fixed hourly schedule.

The usual workflow is:

1. Configure a wallpaper in macOS System Settings.
2. Capture that state with `wallctl`.
3. Repeat for each profile you want.
4. Apply a profile once or activate a collection.

## Install

From this checkout:

```bash
./scripts/install.sh
```

That runs:

```bash
cargo install --path . --locked
```

For development:

```bash
cargo run -- <command>
```

## Interactive Menu

Run `wallctl` with no command to open the menu:

```bash
wallctl
```

The menu can list, inspect, activate, capture, apply, remove, show status, and
print logs. Command help is still available directly:

```bash
wallctl --help
wallctl help
wallctl help use
```

Common commands that need a collection can also open a picker when run in an
interactive terminal:

```bash
wallctl use
wallctl inspect
wallctl apply
wallctl capture
wallctl remove
```

Use exact collection slugs in scripts and non-interactive shells.

## Concepts

A collection is what you activate. It has one strategy:

- `static`: one captured profile.
- `dynamic`: one captured profile that macOS changes itself, such as a dynamic
  HEIC wallpaper.
- `schedule`: multiple captured profiles selected by fixed hour-of-day slots.

A profile is a captured snapshot of the current macOS wallpaper plist. For image
and HEIC wallpapers, `wallctl` copies the referenced asset into the collection
so the profile can keep working if the original file is deleted. For Apple
Aerial wallpapers, it stores a backup of the matching `.mov` when possible.

Collection names become slugs:

```bash
wallctl new static "Focus Mode"
# slug: focus-mode
```

## Common Workflows

### Static Wallpaper

Use this for one saved wallpaper setup.

```bash
wallctl new static "Focus Mode"
wallctl capture focus-mode
wallctl inspect focus-mode
wallctl apply focus-mode
wallctl use focus-mode
```

`apply` applies the profile once. `use` makes the collection active.

### Dynamic Wallpaper

Use this for a macOS-native dynamic wallpaper. If you already have a dynamic
HEIC, set it in System Settings first, then capture it:

```bash
wallctl new dynamic "Day Night"
wallctl capture day-night
wallctl use day-night
```

To create a light/dark HEIC from two images:

```bash
wallctl heic \
  --light ~/Pictures/wallpaper-light.png \
  --dark ~/Pictures/wallpaper-dark.png \
  --output ~/Pictures/wallpaper-dynamic.heic
```

The light and dark source images must have the same pixel dimensions.

### Scheduled Wallpaper

Use this for fixed hourly slots.

```bash
wallctl new schedule "Aerial Day" --preset four
```

The `four` preset creates:

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

You can also define the slots yourself instead of using a preset:

```bash
wallctl new schedule "Work Day" \
  --slot 08:morning \
  --slot 13:afternoon \
  --slot 18:evening
```

Each slot is `HOUR:PROFILE`, where `HOUR` is `0` through `23`. At runtime,
`wallctl` picks the latest slot at or before the current hour. Before the first
slot of the day, it wraps to the last slot from the previous day.

Capture each slot after setting the matching wallpaper in System Settings:

```bash
wallctl capture aerial-day morning
wallctl capture aerial-day day
wallctl capture aerial-day evening
wallctl capture aerial-day night
```

Then activate the schedule:

```bash
wallctl inspect aerial-day
wallctl use aerial-day
```

Activation fails until every scheduled slot has a captured, valid profile.

## Commands

```bash
wallctl list
wallctl status
wallctl inspect [collection-slug]

wallctl new static <name>
wallctl new dynamic <name>
wallctl new schedule <name> --preset three
wallctl new schedule <name> --preset four
wallctl new schedule <name> --slot <hour:profile> [--slot <hour:profile> ...]

wallctl capture [collection-slug] [profile-name]
wallctl apply [collection-slug] [profile-name]
wallctl apply --force <collection-slug> <profile-name>
wallctl use [collection-slug]
wallctl dispatch [--force]

wallctl heic --light <light-image> --dark <dark-image> --output <output.heic>
wallctl heic --light <light-image> --dark <dark-image> --output <output.heic> --force

wallctl logs
wallctl remove [collection-slug]
```

`remove` only works for inactive collections. Activate another collection first
if needed.

## Data Locations

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

Scheduled collections use this per-user LaunchAgent:

```text
~/Library/LaunchAgents/local.wallctl.scheduler.plist
```

## Troubleshooting

If `wallctl status` says the live wallpaper has drifted, reapply the active
collection:

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

If a command asks for an exact slug, run:

```bash
wallctl list
```

If the installed command is not found after `./scripts/install.sh`, add Cargo's
bin directory to your shell path:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

# wallctl

`wallctl` is a macOS wallpaper profile controller. It captures the wallpaper
configuration that macOS is using right now, stores it as a reusable profile,
and can reapply that profile later from the command line or an interactive menu.

The main unit is a collection. A collection can be static, dynamic, or scheduled:
static collections apply one saved profile, dynamic collections let macOS handle
native dynamic wallpapers such as light/dark HEIC files, and scheduled
collections switch between named profiles at fixed hours.
Use `static` for one saved setup, `dynamic` for macOS-managed dynamic wallpaper
files, and `schedule` when `wallctl` should choose profiles by hour.

`wallctl` is designed to keep captured wallpapers durable. Image and HEIC files
referenced by macOS are copied into `wallctl` storage, so profiles can keep
working after the original file is moved or deleted. Apple Aerial video assets
are backed up when possible.

## Install

```bash
./scripts/install.sh
```

For development:

```bash
cargo run -- <command>
```

Run `wallctl` with no command to use the interactive menu. Most commands also
work well directly from the shell, which is useful for scheduled dispatch and
repeatable setup.

## Basic Workflow

The usual flow starts in System Settings. Pick the wallpaper you want, then ask
`wallctl` to capture that live macOS state into a collection. After that, you can
apply the captured profile once with `wallctl apply`, or make the collection the
active wallpaper source with `wallctl use`.

Collection names become slugs, such as `Focus Mode` to `focus-mode`.

Static collection:

```bash
wallctl new static "Focus Mode"
wallctl capture focus-mode
wallctl use focus-mode
```

Dynamic collection:

```bash
wallctl new dynamic "Day Night"
wallctl capture day-night
wallctl use day-night
```

If you have separate light and dark images, create a dynamic HEIC first:

```bash
wallctl heic \
  --light ~/Pictures/wallpaper-light.png \
  --dark ~/Pictures/wallpaper-dark.png \
  --output ~/Pictures/wallpaper-dynamic.heic
```

## Schedules

Schedules are fixed hour slots. Each slot is `HOUR:PROFILE`, using an hour from
`0` through `23`.

Custom schedule:

```bash
wallctl new schedule "Work Day" \
  --slot 08:morning \
  --slot 13:afternoon \
  --slot 18:evening
```

Then set each wallpaper in System Settings and capture it with the matching
profile name:

```bash
wallctl capture work-day morning
wallctl capture work-day afternoon
wallctl capture work-day evening
wallctl use work-day
```

At runtime, `wallctl` picks the latest slot at or before the current hour. Before
the first slot of the day, it wraps to the last slot from the previous day. A
schedule can have at most one slot per hour, so the practical maximum is 24
slots.

Presets are available if you want common defaults:

```bash
wallctl new schedule "Aerial Day" --preset four
```

Preset slots:

```text
three: 06 morning, 10 day, 19 night
four:  06 morning, 10 day, 17 evening, 20 night
```

Activation fails until every slot has a captured, valid profile.

## Commands

```bash
wallctl list
wallctl status
wallctl inspect [collection-slug]

wallctl new static <name>
wallctl new dynamic <name>
wallctl new schedule <name> --slot <hour:profile> [--slot <hour:profile> ...]
wallctl new schedule <name> --preset three
wallctl new schedule <name> --preset four

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

Commands that accept an optional collection open a picker in an interactive
terminal. Use exact slugs in scripts.

## Data

`wallctl` stores collection data in:

```text
~/Library/Application Support/wallctl/
```

Scheduler logs live in:

```text
~/Library/Logs/wallctl/
```

Scheduled collections use:

```text
~/Library/LaunchAgents/local.wallctl.scheduler.plist
```

Captured image and HEIC wallpaper assets are copied into the collection so they
keep working if the original file is deleted. Apple Aerial `.mov` files are
backed up when possible.

## Troubleshooting

If `wallctl status` reports drift, reapply the active collection:

```bash
wallctl use <collection-slug>
```

If a schedule will not activate, run:

```bash
wallctl inspect <collection-slug>
```

Common causes are missing slot captures, missing copied assets, invalid profile
metadata, or an Aerial asset that is missing from both Apple's cache and the
collection backup.

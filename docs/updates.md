# Updates

On launch, Yu'lon checks GitHub Releases for a newer `-Public` release — at most once a day, and
only tags ending `-Public`, so test builds are never offered. If there is one, a line under the
header says so and `See what's new` opens what changed in every public release since yours.
`Later` brings it back next launch; `Skip this version` hides that one version until a newer one
is cut. There is also a `Check for updates` button in the header. Yu'lon remembers all of this in
`update.json` beside `state.json`.

## What "Update now" does

Yu'lon only ever installs a release it can prove: one that publishes a `SHA256SUMS` file listing
the exact file for your computer. Anything else offers the release page instead. The download is
checked against the release's own `SHA256SUMS` before anything is unpacked, and the new build is
started once with a test flag and has to exit cleanly before your install is touched. If any of
that fails, Yu'lon says why and your install is exactly as it was.

| You run | What the button does |
|---|---|
| Linux, the AppImage | Downloads, checks, replaces itself and restarts |
| Linux, the tar.gz folder | Downloads, checks, replaces itself and restarts |
| Windows, the zip folder | Downloads, checks, replaces itself and restarts |
| macOS | Downloads and checks the `.dmg`, then opens it for you to install |
| A folder you cannot write to | Downloads and checks the file into your Downloads folder |
| A source checkout | Opens the release page |

**If two copies of Yu'lon are open**, the one you press Update in will say so and stop. Close the
other one first.

## If an update stops part-way

Yu'lon works in folders called `.yulon-new`, `.yulon-download` and `.yulon-old` in your Yu'lon
folder. All three are removed the next time the new version starts. On Linux and macOS a name
beginning with a dot is hidden by default; on Windows they are ordinary visible folders, so you
will see them while an update runs.

Your own files in the Yu'lon folder are never touched: Yu'lon replaces the files it shipped — the
program and its `_internal` folder — one at a time, and nothing else. If the new version would
have to replace something Yu'lon did not put there, the update stops and tells you, and offers
you the download instead.

The full recovery steps — putting the previous version back by hand — live in
[pylauncher/README.md](../pylauncher/README.md) beside the evidence, and the app's own message bar
quotes them word for word when you need them.

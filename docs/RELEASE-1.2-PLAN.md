# Rill 1.2 development and verification

This is a development plan, not a list of shipped features or supported systems.
Rill 1.1.0 and its published assets remain frozen. New packages and measurements
belong to a separate 1.2 candidate until integration passes.

## Workstreams

| Role | Model | Responsibility |
| --- | --- | --- |
| VM tester A | GPT-6.1 Sol | Debian and Ubuntu family guests, one VM at a time |
| VM tester B | GPT-6.1 Sol | Other distribution families, one VM at a time |
| Compatibility fixer | Gemini Neon | Reproduce and fix reported runtime defects |
| Package builder | Gemini Neon | Native packages, portable archive, reproducible build recipes |
| Browser builder A | Gemini Neon | Tab, navigation, and session improvements |
| Browser builder B | Gemini Neon | Library, settings, accessibility, and page tools |
| Browser critic | Gemini Neon | Independent usefulness, interaction, code, and design review |
| Website builder | Gemini Hydrogen | Public download site and dynamic release resolution |
| Website critic | Gemini Hydrogen | Independent, strict visual and functional review |

Roles run in stages within the available agent slots. There may be no more than
two running guests in total. Guest installations, dependencies, and networking
must not alter the host desktop. The Sol roles test; Neon owns application fixes.

## VM matrix

The current target identities are Debian, Ubuntu, Linux Mint, openEuler, Kali,
Parrot, Pop!_OS, Devuan, Zorin OS, KDE neon, Fedora, Arch, openSUSE,
AlmaLinux, Rocky Linux, Oracle Linux, CentOS Stream, Alpine, NixOS, and Void.
Unavailable official images may require a documented replacement or a blocked
result. Two releases of the same distribution do not count as two identities.
openEuler and Devuan replace the initial elementary OS and MX Linux targets
after official image acquisition failed. Image acquisition alone is not a
compatibility result.

For each identity, record:

- Official image URL, image checksum and verification method, release and kernel.
- Actual VM boot evidence, allocated RAM and vCPUs, and the guest display stack.
- libc, GTK, WebKitGTK, graphics, font, and codec versions where available.
- Exact package filename, artifact SHA-256, source revision, and launch command.
- Installation and uninstall results for the package format being tested.
- First launch, onboarding, navigation, tab/space state, private-tab separation,
  settings persistence, resize, light/dark appearance, and restart behavior.
- Guest screenshots and logs for successful GUI tests; exact error output for
  failures. An SSH session, library inspection, or installed package alone is
  never a browser GUI pass.

Use `passed`, `failed`, `blocked`, and `not tested` distinctly. An Xvfb test
inside a real guest is runtime/GUI evidence for that configuration; it does not
certify that distribution's complete default desktop session. Containers and
chroots are build tools, not substitutes for the requested VM coverage.

## Fix loop

Each issue needs an ID, affected artifact hash and guest, reproduction steps,
observed and expected behavior, severity, owner, and a regression check. The
fixer returns a new candidate. Testers rerun the failing path and related paths
on the new hash before closing it. Successful tests on an older hash remain
historical evidence and must be labeled accordingly.

Compatibility work starts with the existing AppImage's glibc 2.44 floor. Native
packages must be built against an appropriate target runtime; a host binary in
a differently named archive does not establish compatibility. Alpine's musl
and NixOS's runtime layout need explicit handling rather than a claimed generic
Linux pass. Preserve existing profile directories during upgrades.

## Feature acceptance

Aim for approximately 100 useful improvements, without counting the same
behavior once per entry point, package format, test, icon, or preference field.
Builders first inventory current behavior so existing features are not counted
again. Each accepted improvement has a user problem, an accessible discovery
path, implemented behavior, persistence and privacy rules where relevant, and
validation. Keep the minimal default. Avoid new network activity on launch.

Prioritize complete workflows over the count: tab operations and recovery;
session and workspace management; bookmarks and history management; download
handling; page and reading tools; search and keyboard use; reversible
customization; accessibility; and diagnostics that help resolve real problems.
Critics may reject additions that duplicate existing behavior or add complexity
without practical benefit.

The feature ledger must distinguish proposed, implemented, reviewed, verified,
and deferred. A setting with no live effect is unfinished. A test that merely
mirrors the implementation does not establish usefulness. Destructive actions
need clear scope and a recovery path or confirmation appropriate to their cost.

## Website acceptance

The website's first job is to explain Rill and help a Linux user get the right
package. Use real application screenshots and measured claims. Apply the
frontend-design, gemini-frontend-design, and kill-ai-slop skills with a deliberate
visual direction, readable typography, restrained decoration, keyboard access,
reduced-motion support, and mobile layouts.

Resolve the latest published GitHub release at runtime. Identify assets by
format and architecture, link their actual download URLs, and show only available
formats. Handle loading, empty releases, API failures, rate limits, prereleases,
and malformed metadata. Use text-safe DOM rendering. No embedded access token,
hardcoded future package URLs, invented download counts, or automatic shell
execution. Terminal commands must match the selected real asset and be safe to
copy. Keep a usable GitHub Releases fallback without JavaScript.

The Hydrogen critic scores visual craft and perceived human authorship from
1–10, with evidence and concrete defects. A high self-awarded score is not an
acceptance gate. Inspect desktop and mobile renderings, below-the-fold sections,
all download states, keyboard operation, and real link behavior. Send findings
back to the builder, then review the revision independently.

## Release gate

Do not label 1.2 stable based on a feature count. Integrate all accepted changes,
run the Rust suite and relevant interaction tests, inspect packages, and rerun
VM regressions against exact candidate hashes. Publish a coverage matrix that
keeps blocked and untested combinations visible. Retain historical artifacts
and avoid replacing an already published release asset.

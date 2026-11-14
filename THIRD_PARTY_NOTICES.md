# Third-party notices

The [MIT license](LICENSE) applies to this project's source. Dependencies retain
their own licenses; this repository's license does not replace them.

## Three.js

Three.js 0.187.0 is vendored in `web/vendor/` so the browser cockpit runs without
a CDN. Its MIT notice is retained in
[`web/vendor/THREE-LICENSE.txt`](web/vendor/THREE-LICENSE.txt), and the vendored
source retains its upstream headers.

Upstream: <https://github.com/mrdoob/three.js>.

## Slint and other Cargo/npm dependencies

The optional native controller uses Slint 1.18.1. Slint is subject to its own
licensing options. Consult the upstream license before distributing a native
controller binary: <https://slint.dev/pricing> and
<https://github.com/slint-ui/slint/blob/master/LICENSE.md>.

Rust and JavaScript dependencies are recorded in the respective Cargo and npm
lockfiles. Their license notices and terms continue to apply to those packages.
No native controller binaries or signing keys are committed to this repository.

# Item 4 (private-methods-not-enabled multi-class order): NOT implemented, parked 2026-11-01 21:44 PST
Oracle probes: t4.json (45 cases), t4a.json. Shipped model (helper-create-class-features-plugin features.js
shouldTransform, per Class in pre-order, after LWC transformDecorators on the same class):
 decorated+private field -> decorated+private method -> decorators not enabled -> private method not enabled
 (frame at member span) -> static block (frame at class). Rust today: forward() throws NOT_ENABLED /
 static-block globally in step 1, frameless. Plan: do the per-class check in decorators.rs visit_class
 (next to the `fatal` path, framed via edits::Origin), drop Forward::Deferred, frame forward's flag-on get/set.
Known separate divergence: lwcerr-before-deco (export-default replaceWith requeue: shipped visits the
 exported class after later classes).

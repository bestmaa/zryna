//! Exact source-ref-selected provider files; no capture or execution authority.

#[cfg(test)]
mod tests;

pub(in super::super) const LEGACY_PROVIDERS: [&str; 9] = [
    "lib/zryna/bootstrap/limits-v3.mjs",
    "lib/zryna/bootstrap/limits-v4.mjs",
    "lib/zryna/bootstrap/node_modules/@typescript/old/lib/typescript.js",
    "lib/zryna/bootstrap/node_modules/@typescript/old/package.json",
    "lib/zryna/bootstrap/node_modules/@typescript/typescript6/lib/typescript.js",
    "lib/zryna/bootstrap/node_modules/@typescript/typescript6/package.json",
    "lib/zryna/bootstrap/worker-v3.mjs",
    "lib/zryna/bootstrap/worker-v4.mjs",
    "lib/zryna/bootstrap/worker.mjs",
];

pub(in super::super) const PROVIDERS: [&str; 28] = [
    "lib/zryna/bootstrap/limits-v3.mjs",
    "lib/zryna/bootstrap/limits-v4.mjs",
    "lib/zryna/bootstrap/node_modules/@typescript/old/lib/typescript.js",
    "lib/zryna/bootstrap/node_modules/@typescript/old/package.json",
    "lib/zryna/bootstrap/node_modules/@typescript/typescript6/lib/typescript.js",
    "lib/zryna/bootstrap/node_modules/@typescript/typescript6/package.json",
    "lib/zryna/bootstrap/v4/boundary/configuration.mjs",
    "lib/zryna/bootstrap/v4/boundary/dispatch.mjs",
    "lib/zryna/bootstrap/v4/boundary/errors.mjs",
    "lib/zryna/bootstrap/v4/boundary/request.mjs",
    "lib/zryna/bootstrap/v4/boundary/transport.mjs",
    "lib/zryna/bootstrap/v4/syntax/constructions.mjs",
    "lib/zryna/bootstrap/v4/syntax/data-declarations.mjs",
    "lib/zryna/bootstrap/v4/syntax/diagnostics.mjs",
    "lib/zryna/bootstrap/v4/syntax/expression-arena.mjs",
    "lib/zryna/bootstrap/v4/syntax/expressions.mjs",
    "lib/zryna/bootstrap/v4/syntax/functions.mjs",
    "lib/zryna/bootstrap/v4/syntax/imports.mjs",
    "lib/zryna/bootstrap/v4/syntax/matches.mjs",
    "lib/zryna/bootstrap/v4/syntax/names.mjs",
    "lib/zryna/bootstrap/v4/syntax/source.mjs",
    "lib/zryna/bootstrap/v4/syntax/spans.mjs",
    "lib/zryna/bootstrap/v4/syntax/statements.mjs",
    "lib/zryna/bootstrap/v4/syntax/tokens.mjs",
    "lib/zryna/bootstrap/v4/syntax/types.mjs",
    "lib/zryna/bootstrap/worker-v3.mjs",
    "lib/zryna/bootstrap/worker-v4.mjs",
    "lib/zryna/bootstrap/worker.mjs",
];

impl super::Distribution {
    pub(in super::super) fn provider_paths(
        &self,
    ) -> Result<&'static [&'static str], zryna_diagnostics::Diagnostic> {
        if !super::source_ref_matches_version(&self.source.r#ref, &self.version) {
            return Err(super::admission_error("distribution source, version or recipe mismatch"));
        }
        Ok(if self.source.r#ref == "refs/heads/main" { &PROVIDERS } else { &LEGACY_PROVIDERS })
    }
}

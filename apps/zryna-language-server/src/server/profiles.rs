use serde::Deserialize;
use serde_json::{Value, json};

use super::{RevisionCompiler, Server};
use crate::{
    coordinates::PositionEncoding,
    params::{
        InitializeParams, decode_params, normalize_root_uri, path_below_root, root_path_from_uri,
        same_trusted_root,
    },
    protocol::{self, Incoming},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AnalysisProfile {
    Scalar,
    ControlFlow,
    DataOwnership,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InitializeOptions {
    zryna_profile: RequestedProfile,
}

#[derive(Deserialize)]
enum RequestedProfile {
    #[serde(rename = "control-flow-v1")]
    ControlFlow,
    #[serde(rename = "data-ownership-v1")]
    DataOwnership,
}

impl AnalysisProfile {
    fn from_options(options: Option<Value>) -> Option<Self> {
        match options {
            None => Some(Self::Scalar),
            Some(value) => {
                serde_json::from_value::<InitializeOptions>(value).ok().map(|options| match options
                    .zryna_profile
                {
                    RequestedProfile::ControlFlow => Self::ControlFlow,
                    RequestedProfile::DataOwnership => Self::DataOwnership,
                })
            }
        }
    }

    const fn analysis_capability(self) -> &'static str {
        match self {
            Self::Scalar => "scalar-v2",
            Self::ControlFlow => "control-flow-v1",
            Self::DataOwnership => "data-ownership-v1",
        }
    }

    const fn formatting_capability(self) -> &'static str {
        match self {
            Self::Scalar => "scalar-format-v1",
            Self::ControlFlow => "control-flow-format-v1",
            Self::DataOwnership => "data-ownership-format-v1",
        }
    }
}

impl<Compiler: RevisionCompiler> Server<Compiler> {
    pub(super) fn path_for_uri(&self, uri: &str) -> Option<String> {
        let root = self.root_uri.as_ref()?;
        path_below_root(root, uri)
    }

    pub(super) fn initialize(&mut self, message: Incoming) -> Vec<Value> {
        let Some(id) = message.id.as_ref() else {
            return Vec::new();
        };
        if self.initialized {
            return vec![protocol::invalid_request()];
        }
        let Some(params) = decode_params::<InitializeParams>(message.params) else {
            return vec![protocol::invalid_params(Some(id))];
        };
        let (Some(root_uri), Some(profile)) = (
            normalize_root_uri(&params.root_uri),
            AnalysisProfile::from_options(params.initialization_options),
        ) else {
            return vec![protocol::invalid_params(Some(id))];
        };
        if profile == AnalysisProfile::DataOwnership {
            let Some(path) = root_path_from_uri(&root_uri) else {
                return vec![protocol::invalid_params(Some(id))];
            };
            if !self
                .root_path
                .as_ref()
                .is_some_and(|configured| same_trusted_root(configured, &path))
                || self.source_root.is_none()
            {
                return vec![protocol::invalid_params(Some(id))];
            }
        }
        self.encoding = PositionEncoding::select(
            params.capabilities.general.as_ref().and_then(|g| g.position_encodings.as_deref()),
        );
        self.root_uri = Some(root_uri);
        self.profile = profile;
        self.initialized = true;
        vec![protocol::response(
            id,
            &json!({
                "capabilities": {
                    "positionEncoding": self.encoding.as_str(),
                    "textDocumentSync": {"openClose":true,"change":1},
                    "definitionProvider": profile == AnalysisProfile::Scalar,
                    "documentFormattingProvider": true,
                    "documentRangeFormattingProvider": true,
                    "experimental": {"zrynaAnalysisProfile":profile.analysis_capability(),
                        "zrynaFormattingProfile":profile.formatting_capability(),
                        "zrynaInstallationProfile":"portable-setup-v1",
                        "zrynaSourceCommit":option_env!("ZRYNA_TOOLING_SOURCE_COMMIT")}
                },
                "serverInfo": {"name":"zryna-language-server","version":env!("CARGO_PKG_VERSION")}
            }),
        )]
    }
}

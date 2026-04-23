pub(crate) trait TestTargetExt {
    fn supported_kind(&self) -> cargo_metadata::TargetKind;
    fn kind_label(&self) -> String;
    fn sort_order(&self) -> u8;
}

impl TestTargetExt for cargo_metadata::Target {
    // We only care about subset of target kinds for the purpose of analysis.
    fn supported_kind(&self) -> cargo_metadata::TargetKind {
        if self.is_kind(cargo_metadata::TargetKind::Lib) {
            cargo_metadata::TargetKind::Lib
        } else if self.is_kind(cargo_metadata::TargetKind::Bin) {
            cargo_metadata::TargetKind::Bin
        } else if self.is_kind(cargo_metadata::TargetKind::Example) {
            cargo_metadata::TargetKind::Example
        } else if self.is_kind(cargo_metadata::TargetKind::Test) {
            cargo_metadata::TargetKind::Test
        } else if self.is_kind(cargo_metadata::TargetKind::Bench) {
            cargo_metadata::TargetKind::Bench
        } else if self.is_kind(cargo_metadata::TargetKind::CustomBuild) {
            cargo_metadata::TargetKind::CustomBuild
        } else {
            cargo_metadata::TargetKind::Unknown("unknown".to_string())
        }
    }

    fn kind_label(&self) -> String {
        self.supported_kind().to_string()
    }

    fn sort_order(&self) -> u8 {
        let supported_kind = self.supported_kind();
        let order = [
            cargo_metadata::TargetKind::Lib,
            cargo_metadata::TargetKind::Bin,
            cargo_metadata::TargetKind::Example,
            cargo_metadata::TargetKind::Test,
            cargo_metadata::TargetKind::Bench,
            cargo_metadata::TargetKind::CustomBuild,
        ];

        order
            .iter()
            .position(|kind| kind == &supported_kind)
            .unwrap_or(order.len()) as u8
    }
}

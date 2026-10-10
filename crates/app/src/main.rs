//! Winrust desktop app. The skinned UI arrives in milestone M3; until then this only reports
//! its version so the workspace, CI and release plumbing have a binary to build.

fn main() {
    println!("{}", banner());
}

fn banner() -> String {
    format!("Winrust {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_names_product_and_version() {
        assert_eq!(
            super::banner(),
            format!("Winrust {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

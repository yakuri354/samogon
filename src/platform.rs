use std::sync::OnceLock;

static PLATFORM: OnceLock<String> = OnceLock::new();

pub fn get_current_platform() -> &'static str {
    PLATFORM.get_or_init(|| {
        let os = os_info::get();

        match os.os_type() {
            os_info::Type::Macos => {
                let version = match os.version() {
                    os_info::Version::Semantic(maj, min, _) => match maj {
                        14 => "sonoma",
                        13 => "ventura",
                        12 => "monterey",
                        11 => "big_sur",
                        10 => match min {
                            15 => "catalina",
                            14 => "mojave",
                            13 => "high_sierra",
                            12 => "sierra",
                            11 => "el_capitan",
                            10 => "yosemite",
                            _ => panic!("Unsupported macOS version"),
                        },
                        _ => panic!("Unsupported macOS version"),
                    },
                    _ => panic!("Unsupported macOS version"),
                };

                match os.architecture().expect("Unsupported architecture") {
                    "arm64" => format!("arm64_{}", version),
                    "x86_64" => version.to_string(),
                    _ => panic!("Unsupported architecture"),
                }
            }
            _ => panic!("Only macOS is supported"),
        }
    })
}

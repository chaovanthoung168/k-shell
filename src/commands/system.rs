use std::io::{self, Write};

use crate::{
    commands::OutputFormat,
    error::ShellError,
    platform::system::{self, Capacity, SystemSpecs, Volume},
    presentation::Style,
};

pub(super) fn run(
    args: &[String],
    output: &mut dyn Write,
    format: OutputFormat,
) -> Result<(), ShellError> {
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        return writeln!(output, "usage: {}\nShow OS, current user, CPU, display adapters, physical RAM and local fixed-volume space.\nA snapshot; run again to refresh. Alias: sysinfo / specs.", args[0])
            .map_err(|error| ShellError::new(format!("{}: {error}", args[0]), 1));
    }
    if args.len() != 1 {
        return Err(ShellError::new(format!("usage: {} [--help]", args[0]), 2));
    }
    render(&system::collect()?, output, format)
        .map_err(|error| ShellError::new(format!("{}: {error}", args[0]), 1))
}

fn safe_text(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn row(output: &mut dyn Write, label: &str, value: &str, format: OutputFormat) -> io::Result<()> {
    Style::Directory.write_in(
        output,
        &format!("  {:<9}", safe_text(label)),
        format.color && format.terminal_width.is_some(),
        format.appearance.palette,
    )?;
    writeln!(output, "{}", safe_text(value))
}

fn bytes(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}

fn capacity(capacity: &Result<Capacity, String>, ram: bool) -> String {
    match capacity {
        Ok(capacity) if ram => format!(
            "{} used / {} total ({} available)",
            bytes(capacity.total.saturating_sub(capacity.available)),
            bytes(capacity.total),
            bytes(capacity.available),
        ),
        Ok(capacity) => format!(
            "{} total / {} available",
            bytes(capacity.total),
            bytes(capacity.available),
        ),
        Err(error) => format!("Unavailable ({error})"),
    }
}

fn storage_total(volumes: &[Volume]) -> Result<Capacity, String> {
    volumes.iter().try_fold(
        Capacity {
            total: 0,
            available: 0,
        },
        |sum, volume| {
            let capacity = volume
                .capacity
                .as_ref()
                .map_err(|_| "one or more volumes could not be read".to_owned())?;
            match (
                sum.total.checked_add(capacity.total),
                sum.available.checked_add(capacity.available),
            ) {
                (Some(total), Some(available)) => Ok(Capacity { total, available }),
                _ => Err("combined storage exceeds the supported size".to_owned()),
            }
        },
    )
}

fn render(specs: &SystemSpecs, output: &mut dyn Write, format: OutputFormat) -> io::Result<()> {
    Style::Success.write_in(
        output,
        "System specifications\n",
        format.color && format.terminal_width.is_some(),
        format.appearance.palette,
    )?;
    let os = match &specs.os {
        Ok(os) => format!(
            "{}{} (build {}{})",
            os.name,
            os.release
                .as_ref()
                .map(|r| format!(" {r}"))
                .unwrap_or_default(),
            os.build,
            os.revision.map(|r| format!(".{r}")).unwrap_or_default()
        ),
        Err(error) => format!("Unavailable ({error})"),
    };
    row(output, "OS", &os, format)?;
    row(
        output,
        "User",
        &specs
            .user
            .as_ref()
            .cloned()
            .unwrap_or_else(|e| format!("Unavailable ({e})")),
        format,
    )?;
    let cpu = specs
        .cpu
        .as_ref()
        .cloned()
        .unwrap_or_else(|e| format!("Unavailable ({e})"));
    row(output, "CPU", &cpu, format)?;
    if let Some(count) = specs.logical_processors {
        row(
            output,
            "",
            &format!("{count} logical processors available"),
            format,
        )?;
    }
    if specs.gpus.is_empty() {
        row(output, "GPU", "No display adapters reported", format)?;
    } else {
        for (i, gpu) in specs.gpus.iter().enumerate() {
            row(output, if i == 0 { "GPU" } else { "" }, gpu, format)?;
        }
    }
    row(output, "RAM", &capacity(&specs.memory, true), format)?;
    Style::Muted.write_in(
        output,
        "  Storage (local fixed volumes)\n",
        format.color && format.terminal_width.is_some(),
        format.appearance.palette,
    )?;
    match &specs.volumes {
        Ok(volumes) if volumes.is_empty() => row(output, "", "None reported", format)?,
        Ok(volumes) => {
            for volume in volumes {
                row(
                    output,
                    &volume.root,
                    &capacity(&volume.capacity, false),
                    format,
                )?;
            }
            row(
                output,
                "Total",
                &capacity(&storage_total(volumes), false),
                format,
            )?;
        }
        Err(error) => row(output, "", &format!("Unavailable ({error})"), format)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::system::Volume;

    fn sample() -> SystemSpecs {
        SystemSpecs {
            os: Ok(system::OperatingSystem {
                name: "Windows 11 Pro".into(),
                release: Some("24H2".into()),
                build: 26_100,
                revision: Some(1234),
            }),
            user: Ok("Sample ឯក user".into()),
            cpu: Ok("Example CPU".into()),
            logical_processors: Some(8),
            gpus: vec!["Integrated GPU".into(), "Discrete GPU".into()],
            memory: Ok(Capacity {
                total: 16 << 30,
                available: 6 << 30,
            }),
            volumes: Ok(vec![Volume {
                root: "C:\\".into(),
                capacity: Ok(Capacity {
                    total: 512 << 30,
                    available: 200 << 30,
                }),
            }]),
        }
    }

    #[test]
    fn snapshot_preserves_all_adapters_and_formats_real_binary_units() {
        let mut out = Vec::new();
        render(&sample(), &mut out, OutputFormat::default()).unwrap();
        let text = String::from_utf8(out).unwrap();
        for expected in [
            "OS       Windows 11 Pro 24H2 (build 26100.1234)",
            "User     Sample ឯក user",
            "Example CPU",
            "8 logical processors",
            "Integrated GPU",
            "Discrete GPU",
            "10.0 GiB used / 16.0 GiB total (6.0 GiB available)",
            "512.0 GiB total / 200.0 GiB available",
        ] {
            assert!(text.contains(expected), "{text}");
        }
        assert!(!text.contains('\x1b'));
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1 << 40), "1.0 TiB");
    }

    #[test]
    fn identity_rows_handle_missing_optional_values_unavailable_fields_and_controls() {
        let mut specs = sample();
        specs.os.as_mut().unwrap().release = None;
        specs.os.as_mut().unwrap().revision = None;
        specs.user = Err("user access denied".into());
        let mut output = Vec::new();
        render(&specs, &mut output, OutputFormat::default()).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("OS       Windows 11 Pro (build 26100)"));
        assert!(text.contains("User     Unavailable (user access denied)"));
        assert!(text.contains("Example CPU"));
        specs.os = Err("OS registry unavailable".into());
        specs.user = Ok("ឯក\x1b[2J\nuser".into());
        let mut output = Vec::new();
        render(&specs, &mut output, OutputFormat::default()).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("OS       Unavailable (OS registry unavailable)"));
        assert!(text.contains("User     ឯក\\u{1b}[2J\\nuser"));
        assert!(!text.contains('\x1b'));
        assert!(text.contains("512.0 GiB total"));
    }

    #[test]
    fn unavailable_sections_do_not_hide_healthy_sections_or_inject_terminal_controls() {
        let mut specs = sample();
        specs.cpu = Ok("CPU\x1b[2J".into());
        specs.gpus.clear();
        specs.memory = Err("access denied".into());
        specs.volumes.as_mut().unwrap().push(Volume {
            root: "D:\\".into(),
            capacity: Err("not ready".into()),
        });
        let mut out = Vec::new();
        render(&specs, &mut out, OutputFormat::default()).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("CPU\\u{1b}[2J"));
        assert!(text.contains("No display adapters reported"));
        assert!(text.contains("Unavailable (access denied)"));
        assert!(text.contains("512.0 GiB"));
        assert!(text.contains("Unavailable (not ready)"));
        assert!(text.lines().any(|line| line.contains("Total")
            && line.contains("Unavailable (one or more volumes could not be read)")));
        assert!(!text.contains('\x1b'));
    }

    #[test]
    fn storage_summary_sums_capacity_and_available_bytes_across_drives() {
        let mut specs = sample();
        specs.volumes.as_mut().unwrap().push(Volume {
            root: "D:\\".into(),
            capacity: Ok(Capacity {
                total: 256 << 30,
                available: 100 << 30,
            }),
        });
        let mut output = Vec::new();
        render(&specs, &mut output, OutputFormat::default()).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("C:\\      512.0 GiB total / 200.0 GiB available"));
        assert!(text.contains("D:\\      256.0 GiB total / 100.0 GiB available"));
        assert!(text.ends_with("  Total    768.0 GiB total / 300.0 GiB available\n"));
        specs.volumes = Ok(Vec::new());
        let mut output = Vec::new();
        render(&specs, &mut output, OutputFormat::default()).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(text.ends_with("None reported\n"));
        assert!(!text.contains("Total"));
    }

    #[test]
    fn storage_summary_reports_overflow_instead_of_wrapping() {
        let volumes = [
            Volume {
                root: "C:\\".into(),
                capacity: Ok(Capacity {
                    total: u64::MAX,
                    available: 0,
                }),
            },
            Volume {
                root: "D:\\".into(),
                capacity: Ok(Capacity {
                    total: 1,
                    available: 0,
                }),
            },
        ];
        assert!(storage_total(&volumes).unwrap_err().contains("exceeds"));
    }

    #[test]
    fn explicit_terminal_format_uses_shared_palette_and_propagates_writer_errors() {
        let mut output = Vec::new();
        render(
            &sample(),
            &mut output,
            OutputFormat {
                color: true,
                terminal_width: Some(80),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(output.starts_with(b"\x1b[1;38;2;129;199;180mSystem specifications"));
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("\x1b[1;38;2;129;184;234m")
        );
        struct Failing;
        impl Write for Failing {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            render(&sample(), &mut Failing, OutputFormat::default())
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}

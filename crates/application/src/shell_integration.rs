//! Detección de la shell remota y construcción de la línea de integración OSC 7.

use tuki_core::ports::ExecOutput;

/// Shell remota detectada a partir de la sonda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Cmd,
    Unknown,
}

/// Sonda ejecutada por un canal `exec` aparte, sin tocar el PTY del usuario.
pub const PROBE_COMMAND: &str = "echo %COMSPEC%::$PSVersionTable::$0::$SHELL";

/// Marcador único que delimita el final de la inyección silenciosa.
pub const READY_MARKER: &[u8] = b"\x1b]777;tuki-ready\x07";

/// Clasifica la shell a partir de stdout y stderr de la sonda.
pub fn classify(output: &ExecOutput) -> ShellKind {
    let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    let combined = format!("{stdout}\n{stderr}");

    if combined.contains("cmd.exe") && !combined.contains("::$0") {
        return ShellKind::Cmd;
    }
    if combined.contains("%comspec%")
        && (combined.contains("psversion") || combined.contains("system.collections.hashtable"))
    {
        return ShellKind::PowerShell;
    }
    if stderr.contains("fish:") {
        return ShellKind::Fish;
    }

    // En la sonda, los dos últimos valores separados por `::` son `$0` y `$SHELL`.
    let tail = |suffix: &str| {
        stdout
            .rsplit("::")
            .filter(|part| !part.is_empty() && !part.contains('%'))
            .any(|part| part.trim_end().ends_with(suffix))
    };
    if tail("zsh") {
        return ShellKind::Zsh;
    }
    if tail("bash") {
        return ShellKind::Bash;
    }
    ShellKind::Unknown
}

/// Construye la línea de integración para la shell dada; `Unknown` no se inyecta.
pub fn integration_line(shell: ShellKind) -> Option<String> {
    let line = match shell {
        ShellKind::Bash => concat!(
            r#" __tuki_osc7() { printf '\033]7;file://%s%s\007' "${HOSTNAME:-}" "$PWD"; }"#,
            r#"; export PROMPT_COMMAND="__tuki_osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}""#,
            r#"; __tuki_osc7; history -d $((HISTCMD-1)) 2>/dev/null; printf '\r\033[K'; printf '\033]777;tuki-ready\007'"#,
            "\r",
        ),
        ShellKind::Zsh => concat!(
            r#" setopt HIST_IGNORE_SPACE; __tuki_osc7() { printf '\033]7;file://%s%s\007' "${HOST:-}" "$PWD"; }"#,
            r#"; precmd_functions+=(__tuki_osc7); __tuki_osc7; printf '\r\033[K'; printf '\033]777;tuki-ready\007'"#,
            "\r",
        ),
        ShellKind::Fish => concat!(
            r#" function __tuki_osc7 --on-variable PWD; printf '\e]7;file://%s%s\a' (hostname) $PWD; end"#,
            r#"; __tuki_osc7; printf '\r\e[K'; printf '\e]777;tuki-ready\a'"#,
            "\r",
        ),
        ShellKind::PowerShell => concat!(
            r#" Write-Host -NoNewline "`e]7;file:///$($PWD.Path.Replace('\','/'))`a"; $tukiOriginalPrompt = $function:prompt; function global:prompt { Write-Host -NoNewline "`e]7;file:///$($PWD.Path.Replace('\','/'))`a"; & $tukiOriginalPrompt }"#,
            r#"; Clear-History -Count 1 -Newest; Write-Host -NoNewline "`r`e[K`e]777;tuki-ready`a""#,
            "\r",
        ),
        ShellKind::Cmd => concat!(
            r#"echo $E]7;file:///$P$E&prompt $E]7;file:///$P$E\$P$G&echo $E[K&echo $E]777;tuki-ready"#,
            "\u{7}",
            "\r",
        ),
        ShellKind::Unknown => return None,
    };
    Some(line.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuki_core::ports::ExecOutput;

    fn output(stdout: &str, stderr: &str) -> ExecOutput {
        ExecOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            status: Some(0),
        }
    }

    // Salidas sintéticas, reconstruidas de lo que imprime cada shell para PROBE_COMMAND:
    // las POSIX dejan `%COMSPEC%` literal y expanden `$PSVersionTable`, `$0` y `$SHELL`;
    // pwsh muestra la tabla de variables; cmd expande `%COMSPEC%` (el envoltorio de exec
    // consume `$0`, por eso no queda `::$0`); fish avisa de la variable por stderr.
    const BASH: &str = "%COMSPEC%::::/bin/bash::/bin/bash\n";
    const ZSH: &str = "%COMSPEC%::::/usr/local/bin/zsh::/bin/zsh\n";
    const FISH_STDOUT: &str = "%COMSPEC%::::/usr/bin/fish::/usr/bin/fish\n";
    const FISH_STDERR: &str = "fish: $0 is not valid in this context\n";
    const PWSH: &str =
        "%COMSPEC%::Name Value\r\nPSVersion 7.4.0\r\nSystem.Collections.Hashtable\r\n";
    const CMD: &str = "C:\\Windows\\System32\\cmd.exe::$PSVersionTable::$SHELL\r\n";
    const ASH: &str = "%COMSPEC%::::/bin/ash::/bin/ash\n";

    #[test]
    fn should_classify_bash_when_probe_output_ends_in_bash() {
        assert_eq!(classify(&output(BASH, "")), ShellKind::Bash);
    }

    #[test]
    fn should_classify_zsh_when_probe_output_ends_in_zsh() {
        assert_eq!(classify(&output(ZSH, "")), ShellKind::Zsh);
    }

    #[test]
    fn should_classify_fish_when_probe_stderr_has_fish_prefix() {
        assert_eq!(classify(&output(FISH_STDOUT, FISH_STDERR)), ShellKind::Fish);
    }

    #[test]
    fn should_classify_powershell_when_probe_output_has_hashtable() {
        assert_eq!(classify(&output(PWSH, "")), ShellKind::PowerShell);
    }

    #[test]
    fn should_classify_cmd_when_probe_output_has_cmd_exe_without_zero_variable() {
        assert_eq!(classify(&output(CMD, "")), ShellKind::Cmd);
    }

    #[test]
    fn should_classify_unknown_when_probe_output_ends_in_ash() {
        assert_eq!(classify(&output(ASH, "")), ShellKind::Unknown);
    }

    #[test]
    fn should_build_posix_lines_when_shell_is_bash_zsh_or_fish() {
        for shell in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Fish] {
            let line = integration_line(shell).unwrap_or_else(|| panic!("línea para {shell:?}"));
            assert!(line.starts_with(' '), "{shell:?}");
            assert!(line.ends_with('\r'), "{shell:?}");
            assert!(line.contains("]7;file://"), "{shell:?}");
            assert!(line.contains("]777;tuki-ready"), "{shell:?}");
        }
    }

    #[test]
    fn should_build_windows_lines_when_shell_is_powershell_or_cmd() {
        for shell in [ShellKind::PowerShell, ShellKind::Cmd] {
            let line = integration_line(shell).unwrap_or_else(|| panic!("línea para {shell:?}"));
            assert!(line.contains("]7;file:///"), "{shell:?}");
            assert!(line.contains("]777;tuki-ready"), "{shell:?}");
        }
    }

    #[test]
    fn should_return_none_when_shell_is_unknown() {
        assert_eq!(integration_line(ShellKind::Unknown), None);
    }

    #[test]
    fn should_preserve_existing_prompt_command_when_building_bash_line() {
        let line = integration_line(ShellKind::Bash).unwrap();
        assert!(line.contains("__tuki_osc7()"));
        assert!(line.contains("PROMPT_COMMAND"));
        assert!(line.contains("${PROMPT_COMMAND:+;$PROMPT_COMMAND}"));
        assert!(line.contains("history -d $((HISTCMD-1)) 2>/dev/null"));
    }

    #[test]
    fn should_hook_precmd_when_building_zsh_line() {
        let line = integration_line(ShellKind::Zsh).unwrap();
        assert!(line.contains("setopt HIST_IGNORE_SPACE"));
        assert!(line.contains("precmd_functions+=(__tuki_osc7)"));
    }

    #[test]
    fn should_watch_pwd_when_building_fish_line() {
        let line = integration_line(ShellKind::Fish).unwrap();
        assert!(line.contains("function __tuki_osc7 --on-variable PWD"));
        assert!(line.contains("(hostname) $PWD"));
    }

    #[test]
    fn should_wrap_prompt_when_building_powershell_line() {
        let line = integration_line(ShellKind::PowerShell).unwrap();
        assert!(line.contains("$function:prompt"));
        assert!(line.contains("global:prompt"));
        assert!(line.contains(".Replace('\\','/')"));
        assert!(line.contains("Clear-History -Count 1 -Newest"));
    }

    #[test]
    fn should_set_prompt_when_building_cmd_line() {
        let line = integration_line(ShellKind::Cmd).unwrap();
        assert!(line.contains("prompt $E]7;file:///$P$E\\"));
    }

    #[test]
    fn should_emit_ready_marker_before_clearing_the_line_when_building_integration_lines() {
        let cases = [
            (ShellKind::Bash, r"\033[K"),
            (ShellKind::Zsh, r"\033[K"),
            (ShellKind::Fish, r"\e[K"),
            (ShellKind::PowerShell, "$([char]27)[K"),
            (ShellKind::Cmd, "$E[2K"),
        ];
        for (shell, clear) in cases {
            let line = integration_line(shell).unwrap_or_else(|| panic!("línea para {shell:?}"));
            let marker = line
                .find("]777;tuki-ready")
                .unwrap_or_else(|| panic!("marcador para {shell:?}"));
            let clear_at = line
                .find(clear)
                .unwrap_or_else(|| panic!("limpieza para {shell:?}"));
            assert!(
                marker < clear_at,
                "{shell:?}: marcador en {marker}, limpieza en {clear_at}"
            );
        }
    }

    #[test]
    fn should_use_char_casts_when_building_powershell_escapes() {
        let line = integration_line(ShellKind::PowerShell).unwrap();
        assert!(!line.contains("`e"), "Windows PowerShell 5.1 no entiende `e");
        assert!(!line.contains("`a"), "Windows PowerShell 5.1 no entiende `a");
        assert!(line.contains("$([char]27)"));
        assert!(line.contains("$([char]7)"));
    }

    #[test]
    fn should_use_only_one_prompt_command_when_building_cmd_line() {
        let line = integration_line(ShellKind::Cmd).unwrap();
        assert!(line.starts_with("prompt "), "{line}");
        assert!(!line.contains("echo"), "{line}");
        assert!(line.contains("$E]777;tuki-ready$E\\"), "{line}");
    }

    #[test]
    fn should_expose_the_ready_marker_when_emitted_by_the_injected_line() {
        assert_eq!(READY_MARKER, b"\x1b]777;tuki-ready\x07");
    }

    #[test]
    fn should_expose_the_st_ready_marker_when_cmd_emits_it() {
        assert_eq!(READY_MARKER_ST, b"\x1b]777;tuki-ready\x1b\\");
    }
}

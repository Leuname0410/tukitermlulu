//! Detección de la shell remota y construcción de la línea de integración OSC 7.

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
    const FISH_STDERR: &str = "fish: Unknown variable: PSVersionTable\n";
    const PWSH: &str = "%COMSPEC%::Name Value\r\nPSVersion 7.4.0\r\nSystem.Collections.Hashtable\r\n";
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
        assert!(line.contains("prompt $E]7;file:///$P$E\\$P$G"));
    }

    #[test]
    fn should_expose_the_ready_marker_when_emitted_by_the_injected_line() {
        assert_eq!(READY_MARKER, b"\x1b]777;tuki-ready\x07");
    }
}

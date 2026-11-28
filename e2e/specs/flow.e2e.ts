/// <reference types="@wdio/globals/types" />
import { execFile } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { $, browser, expect } from "@wdio/globals";

// La subida solo existe arrastrando desde el SO y WebDriver no puede automatizarla:
// ya está cubierta por tests de integración y queda en el checklist manual T-36.

const run = promisify(execFile);
const delay = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));
const median = (values: readonly number[]): number =>
  [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)] ?? 0;

/**Mide desde que se crea la sesión hasta que se ve el formulario de conexión rápida.*/
async function coldStartMs(): Promise<number> {
  const started = Date.now();
  await browser.reloadSession();
  const form = await $('form[aria-label="Conexión rápida"]');
  await form.waitForExist({ timeout: 20_000 });
  return Date.now() - started;
}

/**Suma las páginas privadas de la app y sus descendientes WebView2 en Windows.*/
async function ramWindowsMb(): Promise<number> {
  const script = [
    "$all = Get-CimInstance Win32_Process",
    "$app = $all | Where-Object { $_.Name -eq 'tuki-desktop.exe' } | Sort-Object CreationDate -Descending | Select-Object -First 1",
    "if (-not $app) { throw 'No se encontró el proceso de la app' }",
    "$tree = @($app)",
    "do {",
    "  $added = $false",
    "  foreach ($p in $all) {",
    "    if ($p.ParentProcessId -in $tree.ProcessId -and $p -notin $tree) { $tree += $p; $added = $true }",
    "  }",
    "} while ($added)",
    "$sum = ($tree | Measure-Object -Property PrivatePageCount -Sum).Sum",
    "[math]::Round($sum / 1MB, 1)",
  ].join("\n");
  const { stdout } = await run("powershell.exe", ["-NoLogo", "-NoProfile", "-Command", script]);
  const value = Number(stdout.trim());
  if (!Number.isFinite(value)) {
    throw new Error(`RAM no válida: ${stdout}`);
  }
  return value;
}

/**Suma el Pss de la app y sus descendientes recorriendo /proc por PPID en Linux.*/
async function ramLinuxMb(): Promise<number> {
  const processes = new Map<number, { pid: number; ppid: number; name: string; pss: number }>();
  for (const entry of await fs.readdir("/proc")) {
    if (!/^\d+$/.test(entry)) {
      continue;
    }
    const pid = Number(entry);
    let status: string;
    let rollup: string;
    try {
      status = await fs.readFile(`/proc/${pid}/status`, "utf8");
      rollup = await fs.readFile(`/proc/${pid}/smaps_rollup`, "utf8");
    } catch {
      continue; // Los procesos efímeros desaparecen mientras se recorre /proc.
    }
    processes.set(pid, {
      pid,
      ppid: Number(status.match(/^PPid:\s+(\d+)$/m)?.[1] ?? 0),
      name: status.match(/^Name:\s+(.+)$/m)?.[1] ?? "",
      pss: Number(rollup.match(/^Pss:\s+(\d+)\s+kB$/m)?.[1] ?? 0) / 1024,
    });
  }
  const candidates = [...processes.values()].filter((item) => item.name.startsWith("tuki-desktop"));
  const root =
    candidates.find((item) => !candidates.some((other) => other.ppid === item.pid)) ??
    candidates.at(0);
  if (root === undefined) {
    throw new Error("No se encontró el proceso de la app");
  }
  const tree = [root];
  let added = true;
  while (added) {
    added = false;
    for (const item of processes.values()) {
      if (!tree.includes(item) && tree.some((parent) => parent.pid === item.ppid)) {
        tree.push(item);
        added = true;
      }
    }
  }
  return Math.round(tree.reduce((sum, item) => sum + item.pss, 0) * 10) / 10;
}

function kpiTable(bootMs: number, ramMb: number | null): string {
  return [
    "| KPI | Medición | Meta | Resultado |",
    "| --- | ---: | ---: | :---: |",
    `| Arranque en frío (mediana de 3) | ${bootMs} ms | < 1000 ms | ${bootMs < 1000 ? "PASS" : "FAIL"} |`,
    `| RAM en reposo (1 sesión) | ${ramMb === null ? "n/d" : `${ramMb.toFixed(1)} MB`} | < 100 MB | ${ramMb !== null && ramMb < 100 ? "PASS" : "FAIL"} |`,
  ].join("\n");
}

/**Enfoca el textarea accesible de xterm; los caracteres llegan al PTY por onData.*/
async function sendTerminal(command: string): Promise<void> {
  await browser.execute(() => {
    const input = document.querySelector('textarea[aria-label="Terminal input"]');
    if (input instanceof HTMLTextAreaElement) {
      input.focus();
    }
  });
  await browser.keys(command);
  await browser.keys("Enter");
}

/**Espera a que el archivo aparezca en el sidebar, refrescando si hace falta.*/
async function waitForEntry(fileName: string): Promise<void> {
  const row = await $(`//table[@aria-label="Entradas remotas"]//tr[contains(., "${fileName}")]`);
  await browser.waitUntil(
    async () => {
      if (await row.isExisting()) {
        return true;
      }
      await (await $('//button[@aria-label="Refrescar"]')).click();
      return false;
    },
    { timeout: 8_000, interval: 2_000, timeoutMsg: `El archivo ${fileName} no apareció en el sidebar` },
  );
}

/**Reenvía `cd` si WebView2 perdió el primer envío de teclado; cada intento exige 5 s.*/
async function followTerminalTo(target: string): Promise<void> {
  const route = await $('input[aria-label="Ruta remota"]');
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    await sendTerminal(`cd ${target}`);
    try {
      await browser.waitUntil(async () => (await route.getValue()) === target, {
        timeout: 5_000,
        timeoutMsg: `El sidebar no siguió la terminal hasta ${target}`,
      });
      return;
    } catch (cause) {
      if (attempt === 3) {
        throw cause;
      }
      console.warn(`[e2e] Reintentando cd ${target}`, cause);
      await delay(2_000);
    }
  }
}

describe("flujo e2e [T-31a]", () => {
  let bootMs: number | undefined;
  let ramPromise: Promise<number | null> | undefined;
  let fileName: string | undefined;
  let localFile: string | undefined;

  after(async () => {
    const table = kpiTable(bootMs ?? 0, ramPromise === undefined ? null : await ramPromise);
    console.log(table);
    const summary = process.env.GITHUB_STEP_SUMMARY;
    if (summary !== undefined) {
      try {
        await fs.appendFile(summary, `## KPIs e2e [T-31a]\n\n${table}\n`);
      } catch (cause) {
        console.warn("[e2e] No se pudo escribir GITHUB_STEP_SUMMARY", cause);
      }
    }
    if (fileName !== undefined) {
      try {
        await sendTerminal(`rm -f /tmp/${fileName}`);
      } catch (cause) {
        console.warn("[e2e] No se pudo borrar el archivo remoto", cause);
      }
    }
    if (localFile !== undefined) {
      try {
        await fs.rm(localFile, { force: true });
      } catch (cause) {
        console.warn("[e2e] No se pudo borrar la descarga", cause);
      }
    }
  });

  it("should connect, follow the terminal folder and download a remote file", async () => {
    const boots = [await coldStartMs(), await coldStartMs(), await coldStartMs()];
    bootMs = median(boots);

    await (
      await $('//form[@aria-label="Conexión rápida"]//label[contains(., "Host")]//input')
    ).setValue("127.0.0.1");
    await (
      await $('//form[@aria-label="Conexión rápida"]//label[contains(., "Puerto")]//input')
    ).setValue("2222");
    await (
      await $('//form[@aria-label="Conexión rápida"]//label[contains(., "Usuario")]//input')
    ).setValue("tuki-bash");
    await (
      await $('//form[@aria-label="Conexión rápida"]//button[normalize-space()="Conectar"]')
    ).click();

    const hostDialog = await $('dialog[aria-label="Huella del host"]');
    const secretDialog = await $('dialog[aria-label="Diálogo de secreto"]');
    await browser.waitUntil(
      async () => (await hostDialog.isExisting()) || (await secretDialog.isExisting()),
      { timeout: 20_000, timeoutMsg: "No apareció la huella ni la petición de contraseña" },
    );
    if (await hostDialog.isExisting()) {
      await (await hostDialog.$('//button[normalize-space()="Aceptar"]')).click();
      await secretDialog.waitForExist({ timeout: 20_000 });
    }
    await (
      await $('//dialog[@aria-label="Diálogo de secreto"]//label[contains(., "Contraseña")]//input[@type="password"]')
    ).setValue("tuki-test");
    await (
      await $('//dialog[@aria-label="Diálogo de secreto"]//button[normalize-space()="Conectar"]')
    ).click();

    const terminal = await $('div[role="region"][aria-label="Terminal"]');
    const sidebar = await $('aside[aria-label="Explorador SFTP"]');
    await browser.waitUntil(async () => (await terminal.isExisting()) && (await sidebar.isExisting()), {
      timeout: 30_000,
      timeoutMsg: "No aparecieron la terminal y el explorador SFTP",
    });
    ramPromise = delay(5_000)
      .then(() => (process.platform === "win32" ? ramWindowsMb() : ramLinuxMb()))
      .catch((cause: unknown) => {
        console.warn("[e2e] No se pudo medir la RAM en reposo", cause);
        return null;
      });

    // La integración OSC 7 puede tardar varios segundos en instalarse tras conectar.
    await delay(12_000);
    await followTerminalTo("/tmp");
    const follow = await $('//label[contains(., "Seguir terminal")]//input[@type="checkbox"]');
    expect(await follow.isSelected()).toBe(true);

    fileName = `e2e-${Date.now()}.txt`;
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      await sendTerminal(`echo e2e > /tmp/${fileName}`);
      try {
        await waitForEntry(fileName);
        break;
      } catch (cause) {
        if (attempt === 3) {
          throw cause;
        }
        console.warn(`[e2e] Reintentando crear ${fileName}`, cause);
      }
    }
    const row = await $(`//table[@aria-label="Entradas remotas"]//tr[contains(., "${fileName}")]`);

    await row.click({ button: "right" });
    const download = await $('//div[@role="menu"]//button[@role="menuitem" and normalize-space()="Descargar"]');
    await download.waitForExist({ timeout: 5_000 });
    await download.click();
    const target = path.join(os.homedir(), "Downloads", fileName);
    localFile = target;
    await fs.rm(target, { force: true });
    await browser.waitUntil(
      async () => {
        try {
          return (await fs.readFile(target, "utf8")).trim() === "e2e";
        } catch {
          return false; // El archivo todavía no ha terminado de descargarse.
        }
      },
      { timeout: 20_000, timeoutMsg: "El archivo no apareció en Descargas con contenido e2e" },
    );
  });
});

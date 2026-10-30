import { chromium } from 'playwright';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { writeFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';

const execFileAsync = promisify(execFile);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function parseArgs(argv) {
  const options = { port: 9224, kind: 'window', seconds: 10, warmup: 3 };
  const allowed = new Set(['port', 'kind', 'title', 'seconds', 'warmup', 'pid', 'output']);
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index]?.replace(/^--/, '');
    if (!allowed.has(key) || !argv[index].startsWith('--') || argv[index + 1] === undefined) {
      throw new Error(`Argumento inválido: ${argv[index]}. Use --port 9224 --kind window --title "Mini Studio Benchmark" --seconds 10 [--pid PID] [--output arquivo.json].`);
    }
    options[key] = argv[index + 1];
  }
  for (const key of ['port', 'seconds', 'warmup', 'pid']) {
    if (options[key] === undefined) continue;
    options[key] = Number(options[key]);
    if (!Number.isFinite(options[key]) || options[key] < (key === 'warmup' ? 0 : 1)) {
      throw new Error(`--${key} deve ser um número ${key === 'warmup' ? 'não negativo' : 'positivo'}.`);
    }
  }
  if (!Number.isInteger(options.port) || options.port > 65535) throw new Error('--port deve ser uma porta válida.');
  if (options.pid !== undefined && !Number.isInteger(options.pid)) throw new Error('--pid deve ser inteiro.');
  if (!['screen', 'window'].includes(options.kind)) throw new Error('--kind deve ser screen ou window; o benchmark não abre câmeras.');
  return options;
}

async function processSnapshot(rootPid) {
  if (!rootPid) return null;
  if (process.platform !== 'win32') throw new Error('--pid só é suportado no Windows.');
  // Apenas PID numérico validado entra no código; nenhum texto do usuário é interpolado.
  const source = `
$ErrorActionPreference = 'Stop'
$benchmarkRoot = ${rootPid}
$inventory = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
$included = [System.Collections.Generic.HashSet[int]]::new()
[void]$included.Add($benchmarkRoot)
do {
  $changed = $false
  foreach ($entry in $inventory) {
    if ($included.Contains([int]$entry.ParentProcessId) -and $included.Add([int]$entry.ProcessId)) { $changed = $true }
  }
} while ($changed)
$rows = @(foreach ($processId in $included) {
  $item = Get-Process -Id $processId -ErrorAction SilentlyContinue
  if ($null -ne $item) {
    try {
      [pscustomobject]@{ pid = $item.Id; name = $item.ProcessName; startTime = $item.StartTime.ToUniversalTime().ToString('o'); cpuSeconds = $item.TotalProcessorTime.TotalSeconds; workingSetBytes = $item.WorkingSet64 }
    } catch {}
  }
})
if (-not ($rows | Where-Object { $_.pid -eq $benchmarkRoot })) { throw 'Processo raiz não encontrado ou inacessível.' }
[pscustomobject]@{ timestampMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); logicalProcessors = [Environment]::ProcessorCount; processes = $rows } | ConvertTo-Json -Depth 4 -Compress
`;
  const encoded = Buffer.from(source, 'utf16le').toString('base64');
  const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-NonInteractive', '-EncodedCommand', encoded], {
    windowsHide: true, timeout: 15_000, maxBuffer: 1024 * 1024,
  });
  return JSON.parse(stdout.replace(/^\uFEFF/, '').trim());
}

function summarizeProcess(start, end) {
  if (!start || !end) return null;
  const before = new Map(start.processes.map((item) => [`${item.pid}:${item.startTime}`, item]));
  const elapsedSeconds = (end.timestampMs - start.timestampMs) / 1000;
  let cpuSeconds = 0;
  const processes = end.processes.map((item) => {
    const previous = before.get(`${item.pid}:${item.startTime}`);
    const delta = Math.max(0, item.cpuSeconds - (previous?.cpuSeconds ?? 0));
    cpuSeconds += delta;
    return { pid: item.pid, name: item.name, cpuSeconds: delta, workingSetMiB: item.workingSetBytes / 1024 ** 2 };
  });
  return {
    elapsedSeconds,
    logicalProcessors: end.logicalProcessors,
    cpuSeconds,
    cpuPercentOneCore: cpuSeconds / elapsedSeconds * 100,
    cpuPercentMachine: cpuSeconds / elapsedSeconds / end.logicalProcessors * 100,
    workingSetMiBStart: start.processes.reduce((sum, item) => sum + item.workingSetBytes, 0) / 1024 ** 2,
    workingSetMiBEnd: end.processes.reduce((sum, item) => sum + item.workingSetBytes, 0) / 1024 ** 2,
    processes,
    exitedPids: start.processes.filter((item) => !end.processes.some((other) => other.pid === item.pid && other.startTime === item.startTime)).map((item) => item.pid),
    note: 'CPU soma processo raiz e descendentes; processos encerrados durante a amostra não contribuem ao delta. Working set somado inclui páginas compartilhadas. A janela de CPU inclui a coleta final via PowerShell.',
  };
}

async function readSample(page) {
  return page.evaluate(() => {
    const text = document.getElementById('fps')?.textContent ?? '';
    return {
      timestamp: performance.now(),
      displayedFps: Number.parseFloat(text.replace(',', '.')) || 0,
      status: document.getElementById('status')?.textContent ?? '',
      metrics: window.__miniStudioMetrics ? JSON.parse(JSON.stringify(window.__miniStudioMetrics)) : null,
    };
  });
}

function summarizeSamples(samples) {
  const start = samples[0];
  const end = samples.at(-1);
  const seconds = (end.timestamp - start.timestamp) / 1000;
  const fps = samples.map((sample) => sample.displayedFps);
  const deltas = {};
  for (const key of ['capturedFrames', 'presentedFrames', 'captureTotalMs', 'decodeTotalMs', 'paintTotalMs']) {
    if (Number.isFinite(start.metrics?.[key]) && Number.isFinite(end.metrics?.[key])) {
      deltas[key] = end.metrics[key] - start.metrics[key];
    }
  }
  return {
    elapsedSeconds: seconds,
    displayedFps: { mean: fps.reduce((sum, value) => sum + value, 0) / fps.length, min: Math.min(...fps), max: Math.max(...fps) },
    telemetryDeltas: deltas,
    capturedFps: deltas.capturedFrames === undefined ? null : deltas.capturedFrames / seconds,
    presentedFps: deltas.presentedFrames === undefined ? null : deltas.presentedFrames / seconds,
    meanCaptureMs: deltas.capturedFrames > 0 ? deltas.captureTotalMs / deltas.capturedFrames : null,
    meanDecodeMs: deltas.capturedFrames > 0 ? deltas.decodeTotalMs / deltas.capturedFrames : null,
    meanPaintMs: deltas.presentedFrames > 0 ? deltas.paintTotalMs / deltas.presentedFrames : null,
  };
}

async function run(options) {
  let browser;
  let page;
  let selectedSource;
  const errors = [];
  try {
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${options.port}`, { timeout: 15_000 });
    for (const candidate of browser.contexts().flatMap((context) => context.pages())) {
      if (await candidate.evaluate(() => Boolean(window.__TAURI__?.core?.invoke && document.getElementById('preview'))).catch(() => false)) {
        page = candidate;
        break;
      }
    }
    if (!page) throw new Error('Nenhuma página do Mini Studio encontrada nessa porta CDP.');
    page.setDefaultTimeout(20_000);
    page.on('pageerror', (error) => errors.push(error.message));
    await page.bringToFront();
    const stop = page.locator('#stop-all');
    if (await stop.isEnabled()) await stop.click();
    await page.locator(options.kind === 'screen' ? '#add-screen' : '#add-window').click();
    const candidates = page.locator('.source-option');
    const selection = options.title ? candidates.filter({ hasText: options.title }).first() : candidates.first();
    await selection.waitFor({ state: 'visible' });
    selectedSource = await selection.locator('strong').textContent();
    await selection.click();
    await page.locator('#source-dialog').waitFor({ state: 'hidden' });
    await sleep(options.warmup * 1000);
    const cpuStart = await processSnapshot(options.pid);
    const samples = [await readSample(page)];
    const deadline = performance.now() + options.seconds * 1000;
    while (performance.now() < deadline) {
      await sleep(Math.min(1000, Math.max(0, deadline - performance.now())));
      samples.push(await readSample(page));
    }
    const cpuEnd = await processSnapshot(options.pid);
    return {
      generatedAt: new Date().toISOString(),
      options,
      selectedSource,
      summary: summarizeSamples(samples),
      process: summarizeProcess(cpuStart, cpuEnd),
      samples,
      errors,
      note: 'Mede a prévia do aplicativo real; não grava imagem nem vídeo. Contadores JS medem processamento/apresentação, não comprovam que o conteúdo da fonte mudou em cada quadro.',
    };
  } finally {
    if (page && !page.isClosed()) {
      try {
        if (await page.locator('#source-dialog').isVisible()) await page.locator('#close-dialog').click();
        if (await page.locator('#stop-all').isEnabled()) await page.locator('#stop-all').click();
      } catch (error) {
        console.error(`Falha ao encerrar a captura: ${error.message}`);
      }
    }
    // Em conexão CDP, close desconecta; não criamos contextos nem fechamos páginas do aplicativo.
    await browser?.close();
  }
}

try {
  const options = parseArgs(process.argv.slice(2));
  const report = await run(options);
  const json = `${JSON.stringify(report, null, 2)}\n`;
  if (options.output) await writeFile(options.output, json, 'utf8');
  process.stdout.write(json);
} catch (error) {
  console.error(JSON.stringify({ error: error.message }));
  process.exitCode = 1;
}

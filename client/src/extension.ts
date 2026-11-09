import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

/**
 * Resolves the path to the strata-lsp executable binary.
 *
 * Checks user configuration, environment variables, workspace target directories,
 * extension bundle targets, ~/.cargo/bin, and system paths.
 */
export function resolveServerBinary(context: vscode.ExtensionContext): string {
  const isWindows = process.platform === 'win32';
  const binaryName = isWindows ? 'strata-lsp.exe' : 'strata-lsp';

  // 1. Explicit user configuration
  const config = vscode.workspace.getConfiguration('strata');
  const configuredPath = config.get<string>('lsp.serverPath');
  if (configuredPath && configuredPath.trim().length > 0) {
    const trimmed = configuredPath.trim();
    if (fs.existsSync(trimmed)) {
      return trimmed;
    }
  }

  // 2. Environment variable override
  const envPath = process.env.STRATA_LSP_PATH;
  if (envPath && envPath.trim().length > 0) {
    const trimmed = envPath.trim();
    if (fs.existsSync(trimmed)) {
      return trimmed;
    }
  }

  // 3. Workspace folder candidate search
  const workspaceFolders = vscode.workspace.workspaceFolders || [];
  for (const folder of workspaceFolders) {
    const candidates = [
      path.join(folder.uri.fsPath, 'target', 'release', binaryName),
      path.join(folder.uri.fsPath, 'target', 'debug', binaryName),
      path.join(folder.uri.fsPath, 'lsp', 'target', 'release', binaryName),
      path.join(folder.uri.fsPath, 'lsp', 'target', 'debug', binaryName),
    ];
    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
  }

  // 4. Extension bundle candidate search
  const extCandidates = [
    path.join(context.extensionPath, 'target', 'release', binaryName),
    path.join(context.extensionPath, 'target', 'debug', binaryName),
    path.join(context.extensionPath, 'lsp', 'target', 'release', binaryName),
    path.join(context.extensionPath, 'lsp', 'target', 'debug', binaryName),
    path.join(context.extensionPath, '..', 'target', 'release', binaryName),
    path.join(context.extensionPath, '..', 'target', 'debug', binaryName),
  ];
  for (const candidate of extCandidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  // 5. Cargo bin directory ($HOME/.cargo/bin/strata-lsp)
  const homeCargoBin = path.join(os.homedir(), '.cargo', 'bin', binaryName);
  if (fs.existsSync(homeCargoBin)) {
    return homeCargoBin;
  }

  // 6. Common Unix system paths
  if (!isWindows) {
    const systemPaths = [
      path.join('/usr', 'local', 'bin', binaryName),
      path.join('/usr', 'bin', binaryName),
      path.join('/opt', 'strata', 'bin', binaryName),
    ];
    for (const candidate of systemPaths) {
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
  }

  // 7. Default to command name for OS PATH lookup
  return binaryName;
}

/**
 * Activates the Strata VS Code extension and starts the Language Server client.
 */
export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const outputChannel = vscode.window.createOutputChannel('Strata Language Server');
  context.subscriptions.push(outputChannel);

  const serverBinary = resolveServerBinary(context);
  outputChannel.appendLine(`Strata Language Server executable resolved: ${serverBinary}`);

  const serverOptions: ServerOptions = {
    run: {
      command: serverBinary,
      transport: TransportKind.stdio,
    },
    debug: {
      command: serverBinary,
      transport: TransportKind.stdio,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'rsx' },
      { scheme: 'file', language: 'rust' },
    ],
    synchronize: {
      fileEvents: [
        vscode.workspace.createFileSystemWatcher('**/*.rsx'),
        vscode.workspace.createFileSystemWatcher('**/*.rs'),
        vscode.workspace.createFileSystemWatcher('**/strata.toml'),
      ],
    },
    outputChannel,
  };

  client = new LanguageClient(
    'strataLsp',
    'Strata Language Server',
    serverOptions,
    clientOptions
  );

  const restartCommand = vscode.commands.registerCommand('strata.restartServer', async () => {
    outputChannel.appendLine('Restarting Strata Language Server...');
    if (client) {
      await client.stop();
    }
    const refreshedBinary = resolveServerBinary(context);
    const newServerOptions: ServerOptions = {
      run: {
        command: refreshedBinary,
        transport: TransportKind.stdio,
      },
      debug: {
        command: refreshedBinary,
        transport: TransportKind.stdio,
      },
    };
    client = new LanguageClient(
      'strataLsp',
      'Strata Language Server',
      newServerOptions,
      clientOptions
    );
    await client.start();
    outputChannel.appendLine('Strata Language Server restarted successfully.');
    vscode.window.showInformationMessage('Strata Language Server restarted.');
  });
  context.subscriptions.push(restartCommand);

  try {
    await client.start();
    outputChannel.appendLine('Strata Language Server started.');
  } catch (error) {
    outputChannel.appendLine(`Failed to start Strata Language Server: ${error}`);
    vscode.window.showErrorMessage(
      `Failed to start Strata Language Server (${serverBinary}). Ensure strata-lsp is built or installed.`
    );
  }
}

/**
 * Deactivates the Strata VS Code extension and terminates the Language Server client.
 */
export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

// Transitional facade over `@/lib/ipc/messaging`: the domain namespaces below
// forward to `sendMessage` / `onMessage` (where the protocol and its schemas now
// live) while call sites move to the messaging API directly.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import type { CommandProtocolMap } from "@/lib/ipc/messaging";
import {
  onWindowCloseRequested,
  readClipboardText,
  setWindowTitle,
  windowLabel,
  writeClipboardText
} from "@/lib/ipc/platform";
import { addRoot, sendSettingsMessage } from "@/lib/settings/settings-requests";
import type {
  ChangeEvent,
  DragDropPayload,
  DragOverPayload,
  McpChange,
  ModelChoice,
  Prefs,
  PtyChunk,
  RunnerData,
  RunnerExit,
  Scheme,
  WindowDirection
} from "@/lib/types";

const DEFAULT_LOG_LIMIT = 20;

type CommandData<Key extends Command> = Parameters<CommandProtocolMap[Key]>[0];

export const agents = {
  detect: () => sendMessage(Command.agentsDetect),
  publishTheme: (scheme: Scheme) => sendMessage(Command.agentThemePublish, { scheme }),
  modelOptions: (command: string) => sendMessage(Command.agentModelOptions, { command })
};

export const ide = {
  detect: () => sendMessage(Command.ideDetect),
  suggest: (cwd: string) => sendMessage(Command.ideSuggest, { cwd }),
  choose: (args: CommandData<Command.ideChooseEditor>) => sendSettingsMessage(Command.ideChooseEditor, args),
  kinds: () => sendMessage(Command.ideKinds),
  kindOptions: () => sendMessage(Command.ideKindOptions),
  projectKinds: (paths: string[]) => sendMessage(Command.ideProjectKinds, { paths }),
  addEditor: (path: string) => sendSettingsMessage(Command.ideAddEditor, { path }),
  removeEditor: (id: string) => sendSettingsMessage(Command.ideRemoveEditor, { id }),
  open: (args: CommandData<Command.ideOpen>) => sendMessage(Command.ideOpen, args),
  openFile: (args: CommandData<Command.ideOpenFile>) => sendMessage(Command.ideOpenFile, args)
};

export const os = {
  explorer: (path: string) => sendMessage(Command.openInExplorer, { path }),
  reveal: (path: string) => sendMessage(Command.revealInExplorer, { path }),
  terminal: (path: string) => sendMessage(Command.openInTerminal, { path }),
  openUrl: (url: string) => sendMessage(Command.openUrl, { url }),
  restart: () => sendMessage(Command.restart)
};

export const clipboard = {
  readText: () => readClipboardText(),
  writeText: (text: string) => writeClipboardText(text),
  saveImage: () => sendMessage(Command.clipboardImageSave)
};

export const recovery = {
  arm: (url: string) => sendMessage(Command.recoveryArm, { url })
};

export const windows = {
  create: (args: CommandData<Command.windowCreate>) => sendMessage(Command.windowCreate, args),
  label: windowLabel,
  setTitle: (title: string) => setWindowTitle(title),
  registerProject: (path: string) => sendMessage(Command.windowRegisterProject, { path }),
  focusProject: (path: string) => sendMessage(Command.windowFocusProject, { path }),
  focusRelative: (direction: WindowDirection) => sendMessage(Command.windowFocusRelative, { direction }),
  list: () => sendMessage(Command.windowList),
  reorder: (labels: string[]) => sendMessage(Command.windowReorder, { labels }),
  onChanged: (handler: () => void) => onMessage(EventType.windowsChanged, () => handler()),
  focus: (label: string) => sendMessage(Command.windowFocusLabel, { label }),
  theme: () => sendMessage(Command.windowTheme),
  onCloseRequested: (handler: () => Promise<void>) => onWindowCloseRequested(handler)
};

export const design = {
  tools: (agent: string) => sendMessage(Command.designTools, { agent }),
  open: (id: string) => sendMessage(Command.designOpen, { id })
};

export const discord = {
  setActivity: ({ details, state, image, caption }: CommandData<Command.discordSetActivity>) =>
    sendMessage(Command.discordSetActivity, {
      details,
      state,
      image,
      caption
    }),
  clearActivity: () => sendMessage(Command.discordClearActivity)
};

export const contextMenu = {
  status: () => sendMessage(Command.contextMenuStatus),
  register: () => sendMessage(Command.contextMenuRegister),
  unregister: () => sendMessage(Command.contextMenuUnregister)
};

export const pty = {
  spawn: (args: CommandData<Command.ptySpawn>) => sendMessage(Command.ptySpawn, args),
  applyModel: (args: {
    sessionId: string;
    command: string;
    conversationId: string;
    choice: ModelChoice;
  }) => sendMessage(Command.agentApplyModel, args),
  write: (args: CommandData<Command.ptyWrite>) => sendMessage(Command.ptyWrite, args),
  resize: (args: CommandData<Command.ptyResize>) => sendMessage(Command.ptyResize, args),
  kill: (id: string) => sendMessage(Command.ptyKill, { id }),
  sessionTaskStop: (args: CommandData<Command.sessionTaskStop>) => sendMessage(Command.sessionTaskStop, args),
  sessionTaskRunning: (args: CommandData<Command.sessionTaskRunning>) =>
    sendMessage(Command.sessionTaskRunning, args),
  list: () => sendMessage(Command.ptyList),
  history: (id: string) => sendMessage(Command.ptyHistory, { id }),
  generateName: (args: CommandData<Command.sessionGenerateName>) => sendMessage(Command.sessionGenerateName, args),
  resumeArgs: (args: CommandData<Command.agentResumeArgs>) => sendMessage(Command.agentResumeArgs, args),
  sessionContext: (args: CommandData<Command.agentSessionContext>) =>
    sendMessage(Command.agentSessionContext, args),
  runningModel: (args: CommandData<Command.agentRunningModel>) => sendMessage(Command.agentRunningModel, args),
  onRunningModelChanged: (callback: (sessionId: string | null) => void) =>
    onMessage(EventType.agentModelChanged, ({ data }) => callback(data.sessionId)),
  onData: (callback: (chunk: PtyChunk) => void) => onMessage(EventType.ptyData, ({ data }) => callback(data)),
  onExit: (callback: (id: string) => void) => onMessage(EventType.ptyExit, ({ data }) => callback(data.id))
};

export const feed = {
  start: (path: string) => sendMessage(Command.watchStart, { root: path }),
  status: () => sendMessage(Command.watchStatus),
  diff: ({ path }: { path: string }) => sendMessage(Command.feedDiff, { path }),
  image: ({ path }: { path: string }) => sendMessage(Command.feedImage, { path }),
  video: ({ path }: { path: string }) => sendMessage(Command.feedVideo, { path }),
  text: ({ path }: { path: string }) => sendMessage(Command.feedText, { path }),
  onChange: (callback: (event: ChangeEvent) => void) =>
    onMessage(EventType.feedChange, ({ data }) => callback(data)),
  onIgnoreChanged: (callback: () => void) => onMessage(EventType.feedIgnoreChanged, () => callback()),
  ignored: (paths: string[]) => sendMessage(Command.feedIgnored, { paths })
};

export const mcp = {
  onChanged: (callback: (change: McpChange) => void) =>
    onMessage(EventType.mcpChanged, ({ data }) => callback(data))
};

export const members = {
  list: (root: string) => sendMessage(Command.membersList, { root })
};

export const dirs = {
  watch: (paths: string[]) => sendMessage(Command.watchDirs, { dirs: paths }),
  onChange: (callback: () => void) => onMessage(EventType.directoriesChanged, () => callback())
};

export const dragDrop = {
  onOver: (callback: (payload: DragOverPayload) => void) =>
    onMessage(EventType.dragOver, ({ data }) => callback(data)),
  onDrop: (callback: (payload: DragDropPayload) => void) =>
    onMessage(EventType.dragDrop, ({ data }) => callback(data)),
  onLeave: (callback: () => void) => onMessage(EventType.dragLeave, () => callback())
};

export const vcs = {
  status: (cwd: string) => sendMessage(Command.vcsStatus, { cwd }),
  log: ({ cwd, limit = DEFAULT_LOG_LIMIT }: {
    cwd: string;
    limit?: number;
  }) => sendMessage(Command.vcsLog, {
    cwd,
    limit
  }),
  diff: ({ cwd, path, staged = false }: {
    cwd: string;
    path: string;
    staged?: boolean;
  }) => sendMessage(Command.vcsDiff, {
    cwd,
    path,
    staged
  }),
  branches: (cwd: string) => sendMessage(Command.vcsBranches, { cwd }),
  branchOf: (paths: string[]) => sendMessage(Command.vcsBranchOf, { paths }),
  remoteBranches: (cwd: string) => sendMessage(Command.vcsRemoteBranches, { cwd }),
  switchBranch: (args: CommandData<Command.vcsSwitchBranch>) => sendMessage(Command.vcsSwitchBranch, args),
  pull: (cwd: string) => sendMessage(Command.vcsPull, { cwd }),
  commit: (args: CommandData<Command.vcsCommit>) => sendMessage(Command.vcsCommit, args),
  commitDiff: ({ cwd, sha, path }: CommandData<Command.vcsCommitDiff>) => sendMessage(Command.vcsCommitDiff, {
    cwd,
    sha,
    path
  }),
  remoteUrl: (cwd: string) => sendMessage(Command.vcsRemoteUrl, { cwd }),
  defaultBranch: (cwd: string) => sendMessage(Command.vcsDefaultBranch, { cwd }),
  gitInstalled: () => sendMessage(Command.vcsGitInstalled),
  hasSshKey: () => sendMessage(Command.vcsHasSshKey),
  probeRemote: (url: string) => sendMessage(Command.vcsProbeRemote, { url }),
  clone: (args: CommandData<Command.vcsClone>) => sendMessage(Command.vcsClone, args),
  worktreeAdd: (args: CommandData<Command.vcsWorktreeAdd>) => sendMessage(Command.vcsWorktreeAdd, args),
  restoreCandidates: (args: CommandData<Command.vcsRestoreCandidates>) =>
    sendMessage(Command.vcsRestoreCandidates, args),
  restoreCheckout: (args: CommandData<Command.vcsRestoreCheckout>) => sendMessage(Command.vcsRestoreCheckout, args),
  onStateChanged: (callback: () => void) => onMessage(EventType.gitState, () => callback())
};

export const runner = {
  start: (args: CommandData<Command.runnerStart>) => sendMessage(Command.runnerStart, args),
  stop: (id: string) => sendMessage(Command.runnerStop, { id }),
  list: () => sendMessage(Command.runnerList),
  onData: (callback: (payload: RunnerData) => void) => onMessage(EventType.runnerData, ({ data }) => callback(data)),
  onExit: (callback: (payload: RunnerExit) => void) => onMessage(EventType.runnerExit, ({ data }) => callback(data))
};

export const tasks = {
  descriptors: () => sendMessage(Command.tasksDescriptors),
  list: (cwd: string) => sendMessage(Command.tasksList, { cwd })
};

export const usage = {
  get: (agent: string) => sendMessage(Command.usageGet, { agent }),
  accountFor: ({ agent, forceRefresh = false }: {
    agent: string;
    forceRefresh?: boolean;
  }) => sendMessage(Command.usageAccountAgent, {
    agent,
    forceRefresh
  }),
  limitResetOffer: () => sendMessage(Command.limitResetOffer),
  claimLimitReset: (grantId: string) => sendMessage(Command.limitResetClaim, { grantId })
};

export const config = {
  list: (agent: string) => sendMessage(Command.configList, { agent }),
  read: (relativePath: string) => sendMessage(Command.configRead, { rel: relativePath })
};

export const terminal = {
  schemes: () => sendMessage(Command.terminalSchemes)
};

export const workspace = {
  context: () => sendMessage(Command.launchContext),
  settings: () => sendSettingsMessage(Command.settingsGet),
  onChanged: (handler: () => void) => onMessage(EventType.settingsChanged, () => handler()),
  addRoot: (args: CommandData<Command.workspaceAddRoot>) => addRoot(args),
  removeRoot: (path: string) => sendSettingsMessage(Command.workspaceRemoveRoot, { path }),
  probePath: (path: string) => sendMessage(Command.workspaceProbePath, { path }),
  scan: (root: string) => sendMessage(Command.workspaceScan, { root }),
  open: (path: string) => sendMessage(Command.workspaceOpen, { path }),
  deleteHandoffDoc: (args: CommandData<Command.handoffDocDelete>) => sendMessage(Command.handoffDocDelete, args),
  temp: () => sendMessage(Command.workspaceTemp),
  move: (args: CommandData<Command.workspaceMove>) => sendMessage(Command.workspaceMove, args),
  rename: (args: CommandData<Command.workspaceRename>) => sendMessage(Command.workspaceRename, args),
  renameFolder: (args: CommandData<Command.workspaceRenameFolder>) => sendMessage(Command.workspaceRenameFolder, args),
  saveMigrate: (args: CommandData<Command.workspaceSaveMigrate>) => sendMessage(Command.workspaceSaveMigrate, args),
  setLabel: (args: CommandData<Command.workspaceSetLabel>) => sendSettingsMessage(Command.workspaceSetLabel, args),
  autoname: (args: CommandData<Command.projectAutoname>) => sendMessage(Command.projectAutoname, args),
  delete: (path: string) => sendSettingsMessage(Command.workspaceDelete, { path }),
  prune: () => sendSettingsMessage(Command.workspacePrune),
  create: (args: CommandData<Command.workspaceCreate>) => sendMessage(Command.workspaceCreate, args),
  clearRecent: () => sendSettingsMessage(Command.workspaceClearRecent),
  setPinned: (args: CommandData<Command.workspaceSetPinned>) => sendSettingsMessage(Command.workspaceSetPinned, args),
  removeRecent: (path: string) => sendSettingsMessage(Command.workspaceRemoveRecent, { path }),
  setPinnedOrder: (paths: string[]) => sendSettingsMessage(Command.workspaceSetPinnedOrder, { paths }),
  deleteDirectory: (path: string) => sendSettingsMessage(Command.workspaceDeleteDirectory, { path }),
  setDefaultAgent: (agent: string) => sendSettingsMessage(Command.setDefaultAgent, { agent }),
  setProjectAgent: (args: CommandData<Command.setProjectAgent>) => sendSettingsMessage(Command.setProjectAgent, args),
  setPrefs: (patch: Partial<Prefs>) => sendSettingsMessage(Command.setPrefs, { patch })
};

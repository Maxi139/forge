import type * as React from 'react';

export type IconName = 'files' | 'file' | 'file-code' | 'folder' | 'folder-open' | 'search' | 'library' | 'board' | 'chip' | 'usb' | 'build' | 'flash' | 'serial' | 'plot' | 'device' | 'settings' | 'command' | 'play' | 'stop' | 'check' | 'check-circle' | 'warning' | 'error' | 'info' | 'close' | 'chevron-down' | 'chevron-right' | 'plus' | 'more' | 'refresh' | 'split' | 'trash' | 'wifi' | 'bug' | 'spark' | 'partition' | 'memory' | 'erase' | 'pause' | 'clock' | 'arrow-down' | 'send' | 'branch';
export type Tone = 'neutral' | 'glut' | 'success' | 'warning' | 'error' | 'info';
export type ConnectionState = 'idle' | 'scanning' | 'connected' | 'compiling' | 'flashing' | 'error';

export interface IconProps { name: IconName; size?: number; label?: string; strokeWidth?: number; className?: string }
export declare function Icon(props: IconProps): React.ReactElement;

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> { variant?: 'primary' | 'secondary' | 'ghost' | 'danger'; size?: 'sm' | 'md' | 'lg'; icon?: IconName; iconRight?: IconName; shortcut?: string; busy?: boolean }
export declare function Button(props: ButtonProps): React.ReactElement;

export interface KbdProps { keys?: string[]; children?: React.ReactNode }
export declare function Kbd(props: KbdProps): React.ReactElement;

export interface BadgeProps { tone?: Tone; icon?: IconName; mono?: boolean; children?: React.ReactNode }
export declare function Badge(props: BadgeProps): React.ReactElement;

export interface StatusDotProps { state?: ConnectionState; label?: React.ReactNode | false }
export declare function StatusDot(props: StatusDotProps): React.ReactElement;

export interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size'> { label?: string; hint?: string; error?: string; icon?: IconName; mono?: boolean; size?: 'sm' | 'md'; trailing?: React.ReactNode }
export declare function Input(props: InputProps): React.ReactElement;

export interface SelectProps extends Omit<React.SelectHTMLAttributes<HTMLSelectElement>, 'size'> { label?: string; hint?: string; options: Array<string | { value: string; label: string }>; icon?: IconName; mono?: boolean; size?: 'sm' | 'md' }
export declare function Select(props: SelectProps): React.ReactElement;

export interface TabItem { id: string; label: string; icon?: IconName; dirty?: boolean; problems?: boolean; count?: number; title?: string }
export interface TabsProps { items: TabItem[]; variant?: 'editor' | 'panel'; value?: string; defaultValue?: string; onChange?(id: string): void; onClose?(id: string): void; actions?: React.ReactNode }
export declare function Tabs(props: TabsProps): React.ReactElement;

export interface PanelProps { title?: React.ReactNode; tabs?: React.ReactNode; actions?: React.ReactNode; padded?: boolean; children?: React.ReactNode; style?: React.CSSProperties }
export declare function Panel(props: PanelProps): React.ReactElement;

export interface TreeNode { name: string; children?: TreeNode[]; open?: boolean; icon?: IconName; status?: 'modified' | 'error'; badge?: React.ReactNode }
export interface ActivityItem { id: string; icon: IconName; label: string; badge?: React.ReactNode }
export interface SidebarProps { nodes?: TreeNode[]; activity?: ActivityItem[]; activityBottom?: ActivityItem[]; view?: string; defaultView?: string; onViewChange?(id: string): void; title?: string; actions?: React.ReactNode; defaultSelected?: string; onSelect?(path: string, node: TreeNode): void; collapsed?: boolean; children?: React.ReactNode }
export declare function Sidebar(props: SidebarProps): React.ReactElement;

export interface StatusItemProps { icon?: IconName; onClick?(): void; title?: string; children?: React.ReactNode }
export interface StatusBarProps { left?: React.ReactNode; right?: React.ReactNode; progress?: number }
export declare function StatusBar(props: StatusBarProps): React.ReactElement;

export interface TitleBarProps { project?: string; board?: string; port?: string; connection?: ConnectionState; phase?: 'idle' | 'compiling' | 'flashing'; searchLabel?: string; windowControls?: boolean; onSearch?(): void; onBoard?(): void; onPort?(): void; onBuild?(): void; onFlash?(): void }
export declare function TitleBar(props: TitleBarProps): React.ReactElement;

export interface Command { id: string; label: string; group?: string; icon?: IconName; hint?: string; shortcut?: string[] }
export interface CommandPaletteProps { items: Command[]; onRun?(item: Command): void; onClose?(): void; placeholder?: string; defaultQuery?: string; autoFocus?: boolean }
export declare function CommandPalette(props: CommandPaletteProps): React.ReactElement;

export interface ToastProps { kind?: 'info' | 'success' | 'warning' | 'error' | 'progress'; title: React.ReactNode; message?: React.ReactNode; actions?: React.ReactNode; onClose?(): void; progress?: number; icon?: IconName }
export declare function Toast(props: ToastProps): React.ReactElement;

export interface DialogProps { title: React.ReactNode; children?: React.ReactNode; footer?: React.ReactNode; icon?: IconName; tone?: 'glut' | 'error' | 'warning'; onClose?(): void; inline?: boolean; width?: number }
export declare function Dialog(props: DialogProps): React.ReactElement;

export interface Diagnostic { line: number; from: number; to: number; severity?: 'error' | 'warning'; message: string }
export interface CompletionItem { label: string; kind?: 'f' | 'v' | 'c' | 't'; detail?: string }
export interface CodeEditorProps { code: string; diagnostics?: Diagnostic[]; cursor?: { line: number; col: number }; completion?: { items: CompletionItem[]; active?: number; query?: string; doc?: string }; breadcrumbs?: string[]; height?: number | string; label?: string }
export declare function CodeEditor(props: CodeEditorProps): React.ReactElement;

export interface MonitorLine { id?: string | number; t?: number; kind?: 'rx' | 'tx' | 'sys' | 'err'; text: string }
export interface SerialMonitorProps { lines: MonitorLine[]; port?: string; baud?: number; state?: ConnectionState; timestamps?: boolean; onSend?(text: string): void; onClear?(): void; onBaud?(baud: number): void; height?: number }
export declare function SerialMonitor(props: SerialMonitorProps): React.ReactElement;

export interface PlotSeries { name: string; values: number[]; unit?: string }
export interface PlotterProps { series: PlotSeries[]; min?: number; max?: number; window?: number; interval?: number; windowLabel?: string; paused?: boolean; onPause?(): void; actions?: React.ReactNode; height?: number }
export declare function Plotter(props: PlotterProps): React.ReactElement;

export interface BuildProgressProps { stage?: number; progress?: number; status?: 'running' | 'done' | 'error'; stages?: string[]; title?: string; target?: string; detail?: React.ReactNode }
export declare function BuildProgress(props: BuildProgressProps): React.ReactElement;

export interface PinoutProps { used?: Record<number, string>; highlight?: number; ledOn?: boolean; legend?: boolean; boardLabel?: string }
export declare function Pinout(props: PinoutProps): React.ReactElement;

export interface MemoryItem { label: string; used: number; total: number; note?: string }
export interface MemoryMeterProps { items: MemoryItem[]; large?: boolean; warnAt?: number; errorAt?: number }
export declare function MemoryMeter(props: MemoryMeterProps): React.ReactElement;

export interface Partition { name: string; type: string; offset: number; size: number; used?: number; color?: 1 | 2 | 3 | 4 | 5 }
export interface PartitionMapProps { partitions: Partition[]; flashSize?: number }
export declare function PartitionMap(props: PartitionMapProps): React.ReactElement;

export interface LibraryItemProps { name: string; version: string; latest?: string; author?: string; downloads?: string; description?: string; state?: 'available' | 'installing' | 'installed' | 'update'; onInstall?(): void; onUpdate?(): void }
export declare function LibraryItem(props: LibraryItemProps): React.ReactElement;

export interface TracesProps { seed?: number; width?: number; height?: number; pitch?: number; count?: number; lit?: number; animate?: boolean; raster?: boolean }
export declare function Traces(props: TracesProps): React.ReactElement;

export interface EmptyStateProps { title: React.ReactNode; text?: React.ReactNode; icon?: IconName; actions?: React.ReactNode; hints?: Array<{ keys: string[]; label: string }>; seed?: number; count?: number; lit?: number; pattern?: boolean; height?: number }
export declare function EmptyState(props: EmptyStateProps): React.ReactElement;

export interface SplashProps { progress?: number; status?: string; version?: string; seed?: number }
export declare function Splash(props: SplashProps): React.ReactElement;

declare global { interface Window { Forge: { Icon: typeof Icon; Button: typeof Button; Kbd: typeof Kbd; Badge: typeof Badge; StatusDot: typeof StatusDot; Input: typeof Input; Select: typeof Select; Tabs: typeof Tabs; Panel: typeof Panel; Sidebar: typeof Sidebar; StatusBar: typeof StatusBar; TitleBar: typeof TitleBar; CommandPalette: typeof CommandPalette; Toast: typeof Toast; Dialog: typeof Dialog; CodeEditor: typeof CodeEditor; SerialMonitor: typeof SerialMonitor; Plotter: typeof Plotter; BuildProgress: typeof BuildProgress; Pinout: typeof Pinout; MemoryMeter: typeof MemoryMeter; PartitionMap: typeof PartitionMap; LibraryItem: typeof LibraryItem; Traces: typeof Traces; EmptyState: typeof EmptyState; Splash: typeof Splash } } }

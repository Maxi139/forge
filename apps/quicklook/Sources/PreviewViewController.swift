import Cocoa
import Quartz
import UniformTypeIdentifiers

/// Ein Eintrag im Dateibaum der Vorschau.
final class Node: NSObject {
    enum Kind { case overview, group, folder, file }

    let kind: Kind
    let title: String
    let entry: ZipEntry?
    /// Pfad relativ zum Bereich (`files/`, `libraries/`), also so, wie die Datei im Projekt heißt.
    let relativePath: String
    var children: [Node] = []

    init(kind: Kind, title: String, entry: ZipEntry? = nil, relativePath: String = "") {
        self.kind = kind
        self.title = title
        self.entry = entry
        self.relativePath = relativePath
    }
}

/// Vorschau für `.forgeproj`: Übersicht, Dateibaum und die Möglichkeit, einzelne Dateien herauszuholen.
final class PreviewViewController: NSViewController, QLPreviewingController, NSOutlineViewDataSource,
    NSOutlineViewDelegate, NSFilePromiseProviderDelegate
{
    private var archive: ZipArchive?
    private var manifest: Manifest?
    private var roots: [Node] = []
    private var overview = Node(kind: .overview, title: "Übersicht")

    private let outline = NSOutlineView()
    private let textView = NSTextView()
    private let imageView = NSImageView()
    private let textScroll = NSScrollView()
    private let saveButton = NSButton(title: "Datei sichern …", target: nil, action: nil)
    private let extractButton = NSButton(title: "Alles entpacken …", target: nil, action: nil)
    private let titleLabel = NSTextField(labelWithString: "")
    private let subtitleLabel = NSTextField(labelWithString: "")
    private let queue = DispatchQueue(label: "cc.forge.quicklook")

    // MARK: Aufbau

    override func loadView() {
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 920, height: 600))
        preferredContentSize = NSSize(width: 920, height: 600)

        titleLabel.font = .systemFont(ofSize: 17, weight: .semibold)
        subtitleLabel.font = .systemFont(ofSize: 12)
        subtitleLabel.textColor = .secondaryLabelColor
        subtitleLabel.lineBreakMode = .byTruncatingTail

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("name"))
        column.resizingMask = .autoresizingMask
        outline.addTableColumn(column)
        outline.outlineTableColumn = column
        outline.headerView = nil
        outline.style = .sourceList
        outline.rowSizeStyle = .medium
        outline.dataSource = self
        outline.delegate = self
        outline.setDraggingSourceOperationMask(.copy, forLocal: false)
        let sidebar = NSScrollView()
        sidebar.documentView = outline
        sidebar.hasVerticalScroller = true
        sidebar.drawsBackground = false

        textView.isEditable = false
        textView.isSelectable = true
        textView.textContainerInset = NSSize(width: 14, height: 12)
        textView.drawsBackground = false
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textScroll.documentView = textView
        textScroll.hasVerticalScroller = true
        textScroll.drawsBackground = false
        imageView.imageScaling = .scaleProportionallyUpOrDown
        imageView.isHidden = true

        let detail = NSView()
        for v in [textScroll, imageView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            detail.addSubview(v)
            NSLayoutConstraint.activate([
                v.leadingAnchor.constraint(equalTo: detail.leadingAnchor),
                v.trailingAnchor.constraint(equalTo: detail.trailingAnchor),
                v.topAnchor.constraint(equalTo: detail.topAnchor),
                v.bottomAnchor.constraint(equalTo: detail.bottomAnchor),
            ])
        }

        let split = NSSplitView()
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebar)
        split.addArrangedSubview(detail)
        sidebar.widthAnchor.constraint(equalToConstant: 250).isActive = true

        saveButton.target = self
        saveButton.action = #selector(saveSelected)
        saveButton.bezelStyle = .rounded
        saveButton.isEnabled = false
        extractButton.target = self
        extractButton.action = #selector(extractAll)
        extractButton.bezelStyle = .rounded
        let hint = NSTextField(labelWithString: "Dateien lassen sich auch aus der Liste auf den Schreibtisch ziehen.")
        hint.font = .systemFont(ofSize: 11)
        hint.textColor = .tertiaryLabelColor
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        let footer = NSStackView(views: [hint, spacer, saveButton, extractButton])
        footer.orientation = .horizontal
        footer.spacing = 8

        for v in [titleLabel, subtitleLabel, split, footer] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        NSLayoutConstraint.activate([
            titleLabel.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 16),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: root.trailingAnchor, constant: -16),
            titleLabel.topAnchor.constraint(equalTo: root.topAnchor, constant: 14),
            subtitleLabel.leadingAnchor.constraint(equalTo: titleLabel.leadingAnchor),
            subtitleLabel.trailingAnchor.constraint(lessThanOrEqualTo: root.trailingAnchor, constant: -16),
            subtitleLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 2),
            split.topAnchor.constraint(equalTo: subtitleLabel.bottomAnchor, constant: 12),
            split.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            split.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            footer.topAnchor.constraint(equalTo: split.bottomAnchor, constant: 10),
            footer.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 16),
            footer.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),
            footer.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -12),
            footer.heightAnchor.constraint(equalToConstant: 24),
        ])
        view = root
    }

    // MARK: Laden

    func preparePreviewOfFile(at url: URL, completionHandler handler: @escaping (Error?) -> Void) {
        queue.async {
            do {
                let zip = try ZipArchive(url: url)
                guard let entry = zip.entry(named: "manifest.json") else { throw ZipError.notAnArchive }
                let manifest = try JSONDecoder().decode(Manifest.self, from: try zip.read(entry))
                DispatchQueue.main.async {
                    self.archive = zip
                    self.manifest = manifest
                    self.build(zip, manifest, fileName: url.deletingPathExtension().lastPathComponent)
                    handler(nil)
                }
            } catch {
                DispatchQueue.main.async { handler(error) }
            }
        }
    }

    private func build(_ zip: ZipArchive, _ m: Manifest, fileName: String) {
        titleLabel.stringValue = m.name
        var bits = [m.kind == "sketch" ? "Sketch" : "Projekt", m.board.fqbn]
        let libs = m.libraries?.count ?? 0
        if libs > 0 { bits.append(libs == 1 ? "1 Bibliothek" : "\(libs) Bibliotheken") }
        if m.ai?.cached == true { bits.append("KI-Prüfung gespeichert") }
        subtitleLabel.stringValue = bits.joined(separator: " · ")

        overview = Node(kind: .overview, title: "Übersicht")
        roots = [overview]
        let sections: [(prefix: String, title: String)] = [
            ("files/", "Dateien"), ("libraries/", "Eingebettete Bibliotheken"), ("ai/", "KI-Gespräche"), ("cache/", "Zwischenspeicher"),
        ]
        for (prefix, title) in sections {
            let group = Node(kind: .group, title: title)
            for e in zip.entries where e.name.hasPrefix(prefix) && !e.isDirectory {
                insert(into: group, entry: e, relative: String(e.name.dropFirst(prefix.count)))
            }
            sortTree(group)
            if !group.children.isEmpty { roots.append(group) }
        }
        outline.reloadData()
        for r in roots where r.kind == .group { outline.expandItem(r, expandChildren: true) }
        outline.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        show(overview)
    }

    private func insert(into group: Node, entry: ZipEntry, relative: String) {
        let parts = relative.split(separator: "/").map(String.init)
        var parent = group
        var walked = ""
        for (i, part) in parts.enumerated() {
            walked += (walked.isEmpty ? "" : "/") + part
            if i == parts.count - 1 {
                parent.children.append(Node(kind: .file, title: part, entry: entry, relativePath: relative))
            } else if let next = parent.children.first(where: { $0.kind == .folder && $0.title == part }) {
                parent = next
            } else {
                let next = Node(kind: .folder, title: part, relativePath: walked)
                parent.children.append(next)
                parent = next
            }
        }
    }

    private func sortTree(_ n: Node) {
        n.children.sort {
            if ($0.kind == .folder) != ($1.kind == .folder) { return $0.kind == .folder }
            return $0.title.localizedStandardCompare($1.title) == .orderedAscending
        }
        n.children.forEach(sortTree)
    }

    // MARK: Liste

    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        (item as? Node)?.children.count ?? roots.count
    }

    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        (item as? Node)?.children[index] ?? roots[index]
    }

    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
        !((item as? Node)?.children.isEmpty ?? true)
    }

    func outlineView(_ outlineView: NSOutlineView, isGroupItem item: Any) -> Bool {
        (item as? Node)?.kind == .group
    }

    func outlineView(_ outlineView: NSOutlineView, shouldSelectItem item: Any) -> Bool {
        (item as? Node)?.kind != .group
    }

    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
        guard let node = item as? Node else { return nil }
        let id = NSUserInterfaceItemIdentifier("cell")
        let cell = (outlineView.makeView(withIdentifier: id, owner: self) as? NSTableCellView) ?? {
            let c = NSTableCellView()
            c.identifier = id
            let icon = NSImageView()
            let label = NSTextField(labelWithString: "")
            label.lineBreakMode = .byTruncatingMiddle
            icon.translatesAutoresizingMaskIntoConstraints = false
            label.translatesAutoresizingMaskIntoConstraints = false
            c.addSubview(icon)
            c.addSubview(label)
            c.imageView = icon
            c.textField = label
            NSLayoutConstraint.activate([
                icon.leadingAnchor.constraint(equalTo: c.leadingAnchor, constant: 2),
                icon.centerYAnchor.constraint(equalTo: c.centerYAnchor),
                icon.widthAnchor.constraint(equalToConstant: 16),
                icon.heightAnchor.constraint(equalToConstant: 16),
                label.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 6),
                label.trailingAnchor.constraint(equalTo: c.trailingAnchor, constant: -4),
                label.centerYAnchor.constraint(equalTo: c.centerYAnchor),
            ])
            return c
        }()
        cell.textField?.stringValue = node.title
        switch node.kind {
        case .group:
            cell.imageView?.image = nil
            cell.textField?.stringValue = node.title.uppercased()
            cell.textField?.font = .systemFont(ofSize: 11, weight: .semibold)
        case .overview:
            cell.imageView?.image = NSImage(systemSymbolName: "info.circle", accessibilityDescription: nil)
            cell.textField?.font = .systemFont(ofSize: 13)
        case .folder:
            cell.imageView?.image = NSImage(systemSymbolName: "folder", accessibilityDescription: nil)
            cell.textField?.font = .systemFont(ofSize: 13)
        case .file:
            cell.imageView?.image = NSWorkspace.shared.icon(
                for: UTType(filenameExtension: (node.title as NSString).pathExtension) ?? .data)
            cell.textField?.font = .systemFont(ofSize: 13)
        }
        return cell
    }

    func outlineViewSelectionDidChange(_ notification: Notification) {
        guard let node = outline.item(atRow: outline.selectedRow) as? Node else { return }
        show(node)
    }

    private var selected: Node? { outline.item(atRow: outline.selectedRow) as? Node }

    // MARK: Anzeige

    private func show(_ node: Node) {
        saveButton.isEnabled = node.kind == .file
        imageView.isHidden = true
        textScroll.isHidden = false
        switch node.kind {
        case .overview:
            textView.textStorage?.setAttributedString(overviewText())
        case .file:
            guard let entry = node.entry, let zip = archive else { return }
            if entry.size > 2_000_000 {
                note("Zu groß für die Vorschau (\(byteCount(entry.size))). Sichere die Datei, um sie zu öffnen.")
                return
            }
            guard let data = try? zip.read(entry) else { return note("Die Datei lässt sich nicht lesen.") }
            if let image = NSImage(data: data), image.isValid, UTType(filenameExtension: (node.title as NSString).pathExtension)?.conforms(to: .image) == true {
                imageView.image = image
                imageView.isHidden = false
                textScroll.isHidden = true
            } else if let text = String(data: data, encoding: .utf8) {
                let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
                textView.textStorage?.setAttributedString(
                    NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: NSColor.labelColor]))
            } else {
                note("Keine Textdatei (\(byteCount(entry.size))). Sichere die Datei, um sie zu öffnen.")
            }
        default:
            break
        }
        textView.scrollToBeginningOfDocument(nil)
    }

    private func note(_ text: String) {
        textView.textStorage?.setAttributedString(NSAttributedString(
            string: text, attributes: [.font: NSFont.systemFont(ofSize: 13), .foregroundColor: NSColor.secondaryLabelColor]))
    }

    private func byteCount(_ n: Int) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(n), countStyle: .file)
    }

    private func overviewText() -> NSAttributedString {
        let out = NSMutableAttributedString()
        func head(_ s: String) {
            out.append(NSAttributedString(string: "\n" + s + "\n", attributes: [
                .font: NSFont.systemFont(ofSize: 11, weight: .semibold), .foregroundColor: NSColor.secondaryLabelColor,
            ]))
        }
        func line(_ s: String, mono: Bool = false) {
            out.append(NSAttributedString(string: s + "\n", attributes: [
                .font: mono ? NSFont.monospacedSystemFont(ofSize: 12, weight: .regular) : NSFont.systemFont(ofSize: 13),
                .foregroundColor: NSColor.labelColor,
            ]))
        }
        guard let m = manifest else { return out }
        if let d = m.description, !d.isEmpty { line(d) }
        head("BOARD")
        line(m.board.fqbn, mono: true)
        for (k, v) in (m.board.options ?? [:]).sorted(by: { $0.key < $1.key }) { line("\(k) = \(v)", mono: true) }
        if let cores = m.toolchain?.cores, !cores.isEmpty {
            head("CORE")
            for (k, v) in cores.sorted(by: { $0.key < $1.key }) { line("\(k) \(v)", mono: true) }
        }
        head("BIBLIOTHEKEN")
        if let libs = m.libraries, !libs.isEmpty {
            for l in libs {
                let how: String
                switch l.source {
                case "embedded": how = "steckt in der Datei"
                case "forge": how = "kommt von Forge"
                default: how = "aus dem Bibliotheksverwalter"
                }
                line("\(l.name) \(l.version) — \(how)")
            }
        } else {
            line("Keine")
        }
        if let secrets = m.secrets, !secrets.isEmpty {
            head("SECRETS (NUR NAMEN, DIE WERTE BLEIBEN IM SCHLÜSSELBUND)")
            for s in secrets { line(s, mono: true) }
        }
        head("KI-PRÜFUNG")
        line(m.ai?.enabled == false ? "Ausgeschaltet" : (m.ai?.cached == true ? "An, Ergebnisse sind gespeichert" : "An, noch keine Ergebnisse"))
        head("GESICHERT")
        var when = ""
        if let t = m.savedAt, t > 0 {
            when = DateFormatter.localizedString(from: Date(timeIntervalSince1970: t), dateStyle: .medium, timeStyle: .short)
        }
        line([when, m.app].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · "))
        return out
    }

    // MARK: Herausholen

    private func write(_ node: Node, to url: URL) throws {
        guard let entry = node.entry, let zip = archive else { return }
        try zip.read(entry).write(to: url, options: .atomic)
    }

    @objc private func saveSelected() {
        guard let node = selected, node.kind == .file else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = node.title
        panel.canCreateDirectories = true
        panel.begin { [weak self] result in
            guard result == .OK, let url = panel.url else { return }
            do { try self?.write(node, to: url) } catch { self?.fail(error) }
        }
    }

    @objc private func extractAll() {
        guard let zip = archive, let m = manifest else { return }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = "Hier entpacken"
        panel.message = "Forge legt einen Ordner „\(m.name)“ an."
        panel.begin { [weak self] result in
            guard result == .OK, let parent = panel.url else { return }
            do {
                let dest = parent.appendingPathComponent(m.name, isDirectory: true)
                for e in zip.entries where e.name.hasPrefix("files/") && !e.isDirectory {
                    guard let rel = safeRelativePath(String(e.name.dropFirst(6))) else { continue }
                    let target = dest.appendingPathComponent(rel)
                    try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
                    try zip.read(e).write(to: target, options: .atomic)
                }
            } catch { self?.fail(error) }
        }
    }

    private func fail(_ error: Error) {
        let alert = NSAlert(error: error)
        alert.runModal()
    }

    // MARK: Ziehen

    func outlineView(_ outlineView: NSOutlineView, pasteboardWriterForItem item: Any) -> NSPasteboardWriting? {
        guard let node = item as? Node, node.kind == .file else { return nil }
        let type = UTType(filenameExtension: (node.title as NSString).pathExtension) ?? .data
        let provider = NSFilePromiseProvider(fileType: type.identifier, delegate: self)
        provider.userInfo = node
        return provider
    }

    func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, fileNameForType fileType: String) -> String {
        (filePromiseProvider.userInfo as? Node)?.title ?? "Datei"
    }

    func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, writePromiseTo url: URL,
                             completionHandler: @escaping (Error?) -> Void)
    {
        guard let node = filePromiseProvider.userInfo as? Node else { return completionHandler(nil) }
        do {
            try write(node, to: url)
            completionHandler(nil)
        } catch {
            completionHandler(error)
        }
    }
}

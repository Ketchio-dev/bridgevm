enum T17FileChooserOwnedSnapshot {
    static func read<Owner, Snapshot>(
        owner: Owner, nodes: (Owner) throws -> [Owner], project: ([Owner]) throws -> Snapshot
    ) throws -> Snapshot {
        try T17FileChooserSnapshot.read(root: { owner }, nodes: nodes, project: project)
    }
}

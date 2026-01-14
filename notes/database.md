The first building block of our database is the Metadata Struct,
This just contains the Metadata for the file including name, size, etc.

then the Metadata Struct is pushed into another tree called Node
that contains the metadata and even more info related to folder/file relantionships.

all the data for the Metadata and Nodes tree are serialized using [#derive(Serialize, DeSerialize)]

(ex.)
``` rust
#[derive(Serialize, Deserialize)]
struct Node {
    id: Uuid,
    name: String,
    kind: NodeKind,
    parent: Option<Uuid>,
    meta: Meta,
    content: Option<Uuid>, // ContentId
    children_count: u32,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    mode: u32,
    uid: u32,
    gid: u32,
    created: i64,
    modified: i64,
    accessed: i64,
    size: u64,
}

enum NodeKind { File, Directory, Symlink }
```

we could store some of these formats in a JSON format and then serialize it.
to take account of changes in the filesystem (files renamed, deleted, moved) we would
use a file-system watcher that would have an event handler that would return a enum everytime
some operation or change occurs on the specified directory. We could have this as a seperate Actor.




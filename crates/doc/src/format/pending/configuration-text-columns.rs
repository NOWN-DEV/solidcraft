//! A configuration table can hold a column for a sketch text's words
//! (`{"text": {"sketch": id, "link": id}}`), so a row sets the design's labels. Older designs
//! have no such column and are unchanged. Readers from the last release refuse the newer
//! format number instead of failing on the unknown column.

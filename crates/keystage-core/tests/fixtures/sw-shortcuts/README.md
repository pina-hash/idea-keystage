# Shortcut import fixtures

Every file here is constructed from the SOLIDWORKS 2025 and 2026 help (see `docs/DECISIONS.md` item 4; research notes in `.work/research/sldreg.md`, "Parser test fixture specification"). None is a real export. All are unvalidated until a real Copy List and a real `.sldreg` from SolidWorks 2025 and 2026 replace them. Command and category names are synthetic (`Fixture ...`), never real SolidWorks names, and no file contains a SolidWorks registry path or value name.

| File                                     | What it stands for                                                                                                                                                                       |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `copylist_tab_header_crlf.txt`           | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. Copy List as tab-separated text with a Category, Command, Shortcut(s) header, UTF-8, CRLF.                   |
| `copylist_tab_noheader_crlf.txt`         | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. The same rows without a header.                                                                              |
| `copylist_tab_header_lf.txt`             | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. LF line endings.                                                                                             |
| `copylist_tab_header_utf16le_bom.txt`    | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. UTF-16LE with BOM `FF FE`.                                                                                   |
| `copylist_tab_header_trailing_blank.txt` | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. A trailing blank line.                                                                                       |
| `copylist_multi_and_bad.txt`             | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. Several shortcuts in one cell, a lowercase shortcut, an unknown key, a two-cell row, and duplicate bindings. |
| `empty.sldreg`                           | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. Zero bytes.                                                                                                  |
| `opaque_binary.sldreg`                   | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. The 64 bytes 0x00 to 0x3F.                                                                                   |
| `backup_userid_20261001_120000.sldreg`   | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. The binary fixture under the documented backup file name pattern.                                            |
| `hypothesis_regedit5_utf8.sldreg`        | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. Hypothesis only: Registration Entries text with a placeholder key, UTF-8.                                    |
| `hypothesis_regedit5_utf16le_bom.sldreg` | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. Hypothesis only: the same text as UTF-16LE with BOM.                                                         |
| `hypothesis_truncated.sldreg`            | Constructed from SOLIDWORKS 2025/2026 help, not a real export. Unvalidated. Hypothesis only: cut in the middle of the key line.                                                          |

Line endings matter here: the `.txt` files must be checked out byte for byte (the tests also build every variant in memory, so they pass either way).

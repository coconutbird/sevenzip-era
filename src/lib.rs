//! 7-Zip plugin for Ensemble ERA archive files.
//!
//! This plugin uses the `sevenzip-plugin` crate to implement ERA archive support
//! for Ensemble Studios games.

use era::{DecryptReader, EncryptWriter, EraArchive, EraWriter, TeaKeys};
use sevenzip_plugin::prelude::*;
use std::io::Cursor;

/// ERA archive format handler.
///
/// ERA is the archive format used by Ensemble Studios games.
/// It uses TEA (Tiny Encryption Algorithm) for encryption.
#[derive(Default)]
pub struct EraFormat {
    /// Parsed archive (for extraction)
    archive: Option<EraArchive<DecryptReader<Cursor<Vec<u8>>>>>,
    /// Raw archive data (needed for editing operations)
    archive_data: Option<Vec<u8>>,
    /// Items in the archive (maps to ERA entries, skipping entry 0)
    items: Vec<EraItem>,
    /// Physical size of the archive
    archive_size: u64,
}

/// Extended item info that tracks the original ERA entry index.
#[derive(Clone)]
struct EraItem {
    /// Standard archive item info
    info: ArchiveItem,
    /// Original index in the ERA archive (entry 0 is filename table)
    era_index: usize,
}

// =============================================================================
// ArchiveFormat implementation
// =============================================================================

impl ArchiveFormat for EraFormat {
    fn name() -> &'static str {
        "ERA"
    }

    fn extension() -> &'static str {
        "era"
    }

    fn class_id() -> [u8; 16] {
        // Custom GUID for ERA format: {12345678-ABCD-EF01-2345-6789ABCDEF01}
        // Same as the original plugin's CLSID_ERA_HANDLER
        //
        // This is the RAW MEMORY LAYOUT of the GUID:
        // - Data1 (0x12345678) as LE: [0x78, 0x56, 0x34, 0x12]
        // - Data2 (0xABCD) as LE: [0xCD, 0xAB]
        // - Data3 (0xEF01) as LE: [0x01, 0xEF]
        // - Data4: [0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01]
        [
            0x78, 0x56, 0x34, 0x12, // Data1 little-endian
            0xCD, 0xAB, // Data2 little-endian
            0x01, 0xEF, // Data3 little-endian
            0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, // Data4
        ]
    }

    // ERA files are encrypted, so no detectable signature
    fn signature() -> Option<&'static [u8]> {
        None
    }

    fn supports_write() -> bool {
        true
    }

    fn supports_update() -> bool {
        true
    }
}

// =============================================================================
// ArchiveReader implementation
// =============================================================================

impl ArchiveReader for EraFormat {
    fn open(&mut self, data: &[u8]) -> Result<()> {
        self.archive_size = data.len() as u64;

        // Store raw data for editing operations
        self.archive_data = Some(data.to_vec());

        // Decrypt and parse the ERA archive
        let cursor = Cursor::new(data.to_vec());
        let decrypt_reader = DecryptReader::new(cursor, TeaKeys::default_archive_keys());

        let archive = EraArchive::new(decrypt_reader)
            .map_err(|e| Error::InvalidFormat(format!("Failed to parse ERA: {:?}", e)))?;

        // Convert entries to items, skipping entry 0 (filename table)
        self.items.clear();
        for (i, entry) in archive.iter().enumerate() {
            if i == 0 {
                continue; // Skip filename table
            }

            let name = entry
                .filename
                .clone()
                .unwrap_or_else(|| format!("entry_{}", i));

            self.items.push(EraItem {
                info: ArchiveItem::file(&name, entry.extra.decomp_size as u64)
                    .with_compressed_size(entry.chunk.size as u64),
                era_index: i,
            });
        }

        self.archive = Some(archive);
        Ok(())
    }

    fn item_count(&self) -> usize {
        self.items.len()
    }

    fn get_item(&self, index: usize) -> Option<&ArchiveItem> {
        self.items.get(index).map(|item| &item.info)
    }

    fn extract(&mut self, index: usize) -> Result<Vec<u8>> {
        let era_index = self
            .items
            .get(index)
            .ok_or(Error::IndexOutOfBounds {
                index,
                count: self.items.len(),
            })?
            .era_index;

        let archive = self
            .archive
            .as_mut()
            .ok_or_else(|| Error::Other("Archive not open".into()))?;

        archive
            .read_entry(era_index)
            .map_err(|e| Error::Other(format!("Failed to read entry {}: {:?}", era_index, e)))
    }

    fn close(&mut self) {
        self.archive = None;
        self.archive_data = None;
        self.items.clear();
        self.archive_size = 0;
    }

    fn physical_size(&self) -> Option<u64> {
        Some(self.archive_size)
    }
}

// =============================================================================
// ArchiveUpdater implementation
// =============================================================================

impl ArchiveUpdater for EraFormat {
    fn update(&mut self, existing_data: &[u8], updates: Vec<UpdateItem>) -> Result<Vec<u8>> {
        // Create a new ERA writer
        let mut era_writer = EraWriter::new();

        // Re-parse the existing archive for reading entries
        let cursor = Cursor::new(existing_data.to_vec());
        let decrypt_reader = DecryptReader::new(cursor, TeaKeys::default_archive_keys());
        let mut source_archive = EraArchive::new(decrypt_reader)
            .map_err(|e| Error::InvalidFormat(format!("Failed to parse source ERA: {:?}", e)))?;

        // Process each update operation
        for update in updates {
            match update {
                UpdateItem::CopyExisting { index, new_name } => {
                    // Get the ERA entry index from our items
                    let era_index = self
                        .items
                        .get(index)
                        .ok_or(Error::IndexOutOfBounds {
                            index,
                            count: self.items.len(),
                        })?
                        .era_index;

                    // Read the entry data
                    let data = source_archive
                        .read_entry(era_index)
                        .map_err(|e| Error::Other(format!("Failed to read entry: {:?}", e)))?;

                    // Get the filename (use new_name if provided, otherwise original)
                    let filename = new_name.unwrap_or_else(|| {
                        self.items
                            .get(index)
                            .map(|item| item.info.name.clone())
                            .unwrap_or_else(|| format!("entry_{}", era_index))
                    });

                    era_writer.add_file(&filename, data);
                }
                UpdateItem::AddNew { name, data } => {
                    era_writer.add_file(&name, data);
                }
            }
        }

        // Write the new ERA archive
        let mut buffer = Cursor::new(Vec::new());
        let keys = TeaKeys::default_archive_keys();
        let encrypt_writer = EncryptWriter::new(&mut buffer, keys);

        era_writer
            .write(encrypt_writer)
            .map_err(|e| Error::Other(format!("Failed to write ERA: {}", e)))?;

        Ok(buffer.into_inner())
    }
}

// =============================================================================
// DLL exports
// =============================================================================

sevenzip_plugin::register_format!(EraFormat, updatable);

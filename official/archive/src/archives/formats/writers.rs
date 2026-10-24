//! Per-format archive writers.
use super::super::{compress::Source, err, ArchiveContext, CANCELLED};
use std::io::{self, Read, Seek, Write};

/// Cancels a long single-file copy between chunks instead of at file boundaries.
struct CancelReader<'a, R> {
    inner: R,
    context: &'a dyn ArchiveContext,
}

impl<R: Read> Read for CancelReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.context.cancelled() {
            return Err(io::Error::other(CANCELLED));
        }
        self.inner.read(buffer)
    }
}

fn total_bytes(sources: &[Source]) -> u64 {
    sources.iter().map(|source| source.size).sum()
}

fn report_progress(context: &dyn ArchiveContext, done: u64, total: u64) {
    context.progress(done, if total > 0 { Some(total) } else { None });
}

pub(crate) fn write_zip<W: Write + Seek>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
    level: u32,
    password: Option<&str>,
) -> Result<W, String> {
    let files = context.files();
    let total = total_bytes(sources);
    let mut done = 0;
    let mut zip = zip::ZipWriter::new(writer);
    for source in sources {
        if context.cancelled() {
            return Err(CANCELLED.into());
        }
        let mut options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(level as i64));
        if let Some(password) = password {
            options = options.with_aes_encryption(zip::AesMode::Aes256, password);
        }
        if source.dir {
            zip.add_directory(format!("{}/", source.name), options)
                .map_err(err)?;
        } else {
            zip.start_file(&source.name, options).map_err(err)?;
            let mut input = CancelReader {
                inner: files.open_read(&source.path).map_err(err)?,
                context,
            };
            io::copy(&mut input, &mut zip).map_err(err)?;
        }
        done += source.size;
        report_progress(context, done, total);
    }
    zip.finish().map_err(err)
}

pub(crate) fn write_tar<W: Write>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
) -> Result<W, String> {
    let files = context.files();
    let total = total_bytes(sources);
    let mut done = 0;
    let mut builder = tar::Builder::new(writer);
    for source in sources {
        if context.cancelled() {
            return Err(CANCELLED.into());
        }
        let mut header = tar::Header::new_gnu();
        header.set_mode(if source.dir { 0o755 } else { 0o644 });
        header.set_size(if source.dir { 0 } else { source.size });
        header.set_entry_type(if source.dir {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_cksum();
        if source.dir {
            builder
                .append_data(&mut header, &source.name, io::empty())
                .map_err(err)?;
        } else {
            builder
                .append_data(
                    &mut header,
                    &source.name,
                    CancelReader {
                        inner: files.open_read(&source.path).map_err(err)?,
                        context,
                    },
                )
                .map_err(err)?;
        }
        done += source.size;
        report_progress(context, done, total);
    }
    builder.into_inner().map_err(err)
}

pub(crate) fn write_tar_gz<W: Write>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
    level: u32,
) -> Result<W, String> {
    write_tar(
        context,
        sources,
        flate2::write::GzEncoder::new(writer, flate2::Compression::new(level)),
    )?
    .finish()
    .map_err(err)
}

pub(crate) fn write_tar_bz2<W: Write>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
    level: u32,
) -> Result<W, String> {
    write_tar(
        context,
        sources,
        bzip2::write::BzEncoder::new(writer, bzip2::Compression::new(level.max(1))),
    )?
    .finish()
    .map_err(err)
}

pub(crate) fn write_tar_xz<W: Write>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
    level: u32,
) -> Result<W, String> {
    write_tar(context, sources, xz2::write::XzEncoder::new(writer, level))?
        .finish()
        .map_err(err)
}

pub(crate) fn write_7z<W: Write + Seek>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
) -> Result<W, String> {
    let files = context.files();
    let total = total_bytes(sources);
    let mut done = 0;
    let mut archive = sevenz_rust2::ArchiveWriter::new(writer).map_err(err)?;
    for source in sources {
        if context.cancelled() {
            return Err(CANCELLED.into());
        }
        if source.dir {
            archive
                .push_archive_entry::<io::Empty>(
                    sevenz_rust2::ArchiveEntry::new_directory(&source.name),
                    None,
                )
                .map_err(err)?;
        } else {
            archive
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(&source.name),
                    Some(CancelReader {
                        inner: files.open_read(&source.path).map_err(err)?,
                        context,
                    }),
                )
                .map_err(err)?;
        }
        done += source.size;
        report_progress(context, done, total);
    }
    archive.finish().map_err(err)
}

pub(crate) fn write_rar<W: Write>(
    context: &dyn ArchiveContext,
    sources: &[Source],
    writer: W,
    level: u32,
    password: Option<&str>,
) -> Result<W, String> {
    let total = total_bytes(sources);
    let mut done = 0;
    let mut builder =
        rars::Builder::new(rars::ArchiveVersion::Rar50).compression_level(Some(level.min(5) as u8));
    if let Some(password) = password {
        builder = builder.password(Some(password.as_bytes().to_vec()));
    }
    for source in sources {
        if context.cancelled() {
            return Err(CANCELLED.into());
        }
        if source.dir {
            builder
                .add_directory(source.name.as_bytes().to_vec(), None, None)
                .map_err(err)?;
        } else {
            let files = context.files();
            let path = source.path.clone();
            let input = rars::EntrySource::from_opener(source.size, move || {
                let reader = files.open_read(&path).map_err(rars::Error::from)?;
                Ok(Box::new(reader))
            });
            builder
                .add_source(source.name.as_bytes().to_vec(), input, None, None)
                .map_err(err)?;
        }
        done += source.size;
        report_progress(context, done, total);
    }
    let mut writer = writer;
    builder
        .write_to(&mut writer, &rars::WriterResources::default(), None)
        .map_err(err)?;
    Ok(writer)
}

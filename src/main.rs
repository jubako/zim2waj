use clap::Parser;

use dropout::Dropper;
use indicatif_log_bridge::LogWrapper;
use jbk::creator::{
    AtomicOutFile, BasicCreator, CompHint, ConcatMode, ContentPackCreator, InputReader,
    PackRecipient,
};
use mime_guess::{mime, Mime};
use rand::seq::SliceRandom;
use rand::thread_rng;
use rayon::prelude::*;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use zim_rs::archive::Archive;

use log::info;

#[inline(always)]
fn spawn<F, T>(name: &'static str, f: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("Success to launch thread")
}
#[derive(Parser)]
#[clap(name = "zim2waj")]
#[clap(author, version, about, long_about=None)]
struct Cli {
    // Input
    #[clap(value_parser)]
    zim_file: PathBuf,

    // Archive name to create
    #[clap(short, long, value_parser)]
    outfile: PathBuf,

    // Split binary content from text
    #[clap(long)]
    split: bool,
}

#[derive(Clone)]
struct ProgressBar {
    pub comp_clusters: indicatif::ProgressBar,
    pub uncomp_clusters: indicatif::ProgressBar,
    pub written_clusters: indicatif::ProgressBar,
    pub entries: indicatif::ProgressBar,
    pub size: indicatif::ProgressBar,
}

impl ProgressBar {
    fn gather_information(zim: &Archive) -> u32 {
        zim.get_all_entrycount()
    }

    fn new(zim: &Archive) -> jbk::Result<Self> {
        let env = env_logger::Env::default()
            .filter("WAJ_LOG")
            .write_style("WAJ_LOG_STYLE");
        let logger = env_logger::Builder::from_env(env)
            .format_timestamp_millis()
            .build();

        let draw_target = indicatif::ProgressDrawTarget::stdout_with_hz(1);

        let multi = indicatif::MultiProgress::with_draw_target(draw_target);
        multi.set_move_cursor(true);

        let nb_entries = Self::gather_information(zim);

        let bytes_style = indicatif::ProgressStyle::with_template(
            "{prefix} : {bytes:7} ({binary_bytes_per_sec})",
        )
        .unwrap();
        let size = indicatif::ProgressBar::new_spinner()
            .with_style(bytes_style)
            .with_prefix("Processed size");
        multi.add(size.clone());

        let cluster_style =
            indicatif::ProgressStyle::with_template("{prefix} : {human_pos} ({human_len})")
                .unwrap();
        let comp_clusters = indicatif::ProgressBar::new(0)
            .with_style(cluster_style.clone())
            .with_prefix("Compressed Cluster");

        let uncomp_clusters = indicatif::ProgressBar::new(0)
            .with_style(cluster_style.clone())
            .with_prefix("Uncompressed Cluster");

        let written_clusters = indicatif::ProgressBar::new(0)
            .with_style(cluster_style.clone())
            .with_prefix("Written clusters");
        multi.add(comp_clusters.clone());
        multi.add(uncomp_clusters.clone());
        multi.add(written_clusters.clone());

        let entries_style = indicatif::ProgressStyle::with_template(
                "{prefix} : {elapsed} / {duration} : [{wide_bar:.cyan/blue}] {human_pos:10} / {human_len:10}"
            )
            .unwrap()
            .progress_chars("#+- ");
        let entries = indicatif::ProgressBar::new(nb_entries as u64)
            .with_style(entries_style)
            .with_prefix("Processed entries");
        multi.add(entries.clone());

        comp_clusters.tick();
        uncomp_clusters.tick();
        written_clusters.tick();

        LogWrapper::new(multi.clone(), logger).try_init().unwrap();
        Ok(Self {
            entries,
            comp_clusters,
            uncomp_clusters,
            written_clusters,
            size,
        })
    }
}

impl jbk::creator::Progress for ProgressBar {
    fn new_cluster(&self, _cluster_idx: u32, compressed: bool) {
        if compressed {
            &self.comp_clusters
        } else {
            &self.uncomp_clusters
        }
        .inc_length(1)
    }

    fn handle_cluster(&self, _cluster_idx: u32, compressed: bool) {
        if compressed {
            &self.comp_clusters
        } else {
            &self.uncomp_clusters
        }
        .inc(1)
    }

    fn handle_cluster_written(&self, _cluster_idx: u32) {
        self.written_clusters.inc(1)
    }

    fn content_added(&self, size: jbk::Size) {
        self.size.inc(size.into_u64())
    }
}

pub struct ContentAdderSwitch {
    content_pack: BasicCreator,
    binary_content_pack: Option<ContentPackCreator<dyn PackRecipient + 'static>>,
}

impl ContentAdderSwitch {
    fn new(
        content_pack: BasicCreator,
        binary_content_pack: Option<ContentPackCreator<dyn PackRecipient + 'static>>,
    ) -> Self {
        Self {
            content_pack,
            binary_content_pack,
        }
    }

    fn add_content(
        &mut self,
        reader: Box<dyn InputReader>,
        is_binary: bool,
        comp_hint: CompHint,
    ) -> jbk::Result<jbk::ContentAddress> {
        if let Some(binary_content_pack) = &mut self.binary_content_pack {
            if is_binary {
                return binary_content_pack.add_content(reader, comp_hint);
            }
        }
        self.content_pack.add_content(reader, comp_hint)
    }

    fn into_inner(
        self,
    ) -> (
        BasicCreator,
        Vec<ContentPackCreator<dyn PackRecipient + 'static>>,
    ) {
        (
            self.content_pack,
            self.binary_content_pack.into_iter().collect(),
        )
    }
}

pub struct Converter {
    content_adder_switch: ContentAdderSwitch,
    entry_store_creator: Box<waj::create::EntryStoreCreator>,
    progress: Arc<ProgressBar>,
    has_main_page: bool,
    dropper: Dropper<Droppable>,
}

enum ZimEntryKind {
    Redirect(String),
    Content(jbk::ContentAddress, Mime),
}

struct ZimEntry {
    path: String,
    data: ZimEntryKind,
    is_main: bool,
}

impl ZimEntry {
    pub fn new(
        entry: zim_rs::entry::Entry,
        dropper: &Dropper<Droppable>,
        adder: &mut ContentAdderSwitch,
    ) -> jbk::Result<Self> {
        let path = entry.get_path();
        let is_main = path.is_empty();
        let path = path.strip_prefix('/').unwrap_or(&path);
        Ok(if entry.is_redirect() {
            Self::new_redirect(
                path.into(),
                entry.get_redirect_entry().unwrap().get_path(),
                is_main,
            )
        } else {
            let item = entry.get_item(false).unwrap();
            dropper.dropout(entry.into());
            let item_mimetype = item.get_mimetype().unwrap();
            let is_binary = !(item_mimetype.contains("text/")
                || item_mimetype.contains("javascript")
                || item_mimetype.contains("json")
                || item_mimetype.contains("image/svg")
                || item_mimetype.contains("xml"));
            let item_size = item.get_size();
            let direct_access = item.get_direct_access().unwrap();
            let comp_hint = if direct_access.is_some() {
                CompHint::No
            } else {
                CompHint::Yes
            };
            let reader: Box<dyn InputReader> =
                if direct_access.is_none() || item_size <= 4 * 1024 * 1024 {
                    Box::new(std::io::Cursor::new(item.get_data().unwrap()))
                } else {
                    let direct_access = direct_access.unwrap();
                    Box::new(jbk::creator::InputFile::new_range(
                        std::fs::File::open(direct_access.get_path())?,
                        direct_access.get_offset(),
                        Some(item_size),
                    )?)
                };
            let content_address = adder.add_content(reader, is_binary, comp_hint)?;
            dropper.dropout(item.into());
            Self {
                path: path.into(),
                data: ZimEntryKind::Content(
                    content_address,
                    Mime::from_str(&item_mimetype).unwrap_or({
                        /*println!(
                            "{}: {} is not a valid mime type. Using mime::APPLICATION_OCTET_STREAM",
                            entry.get_path(),
                            &item_mimetype
                        );*/
                        mime::APPLICATION_OCTET_STREAM
                    }),
                ),
                is_main,
            }
        })
    }
    pub fn new_redirect(path: String, target: String, is_main: bool) -> Self {
        Self {
            path,
            data: ZimEntryKind::Redirect(target),
            is_main,
        }
    }
}

impl waj::create::EntryTrait for ZimEntry {
    fn kind(&self) -> jbk::Result<Option<waj::create::EntryKind>> {
        Ok(Some(match &self.data {
            ZimEntryKind::Redirect(target) => waj::create::EntryKind::Redirect(target.clone()),
            ZimEntryKind::Content(content_address, mime) => {
                waj::create::EntryKind::Content(*content_address, mime.clone())
            }
        }))
    }

    fn name(&self) -> Cow<str> {
        Cow::Borrowed(&self.path)
    }
}

#[allow(dead_code)]
enum Droppable {
    Blob(zim_rs::blob::Blob),
    Entry(zim_rs::entry::Entry),
    Item(zim_rs::item::Item),
}

impl From<zim_rs::blob::Blob> for Droppable {
    fn from(value: zim_rs::blob::Blob) -> Self {
        Self::Blob(value)
    }
}
impl From<zim_rs::entry::Entry> for Droppable {
    fn from(value: zim_rs::entry::Entry) -> Self {
        Self::Entry(value)
    }
}
impl From<zim_rs::item::Item> for Droppable {
    fn from(value: zim_rs::item::Item) -> Self {
        Self::Item(value)
    }
}

fn entry_producer(
    zim: Arc<Archive>,
    dropper: Dropper<Droppable>,
) -> std::sync::mpsc::Receiver<zim_rs::entry::Entry> {
    let (tx, rx) = std::sync::mpsc::sync_channel(2048);

    spawn("entry producer", move || {
        let iter = zim.iter_efficient().unwrap();
        let filter = if zim.has_new_namespace_scheme() {
            |_p: &str| true
        } else {
            |p: &str| matches!(&p.as_bytes()[0], b'-' | b'A' | b'C' | b'J' | b'I')
        };
        let mut redirect_idx = vec![];
        let mut entries_idx = iter
            .into_iter()
            .filter_map(|e| {
                let e = e.unwrap();
                let ret = if filter(&e.get_path()) {
                    if e.is_redirect() {
                        redirect_idx.push(e.get_index());
                        None
                    } else {
                        Some(e.get_index())
                    }
                } else {
                    None
                };
                dropper.dropout(e.into());
                ret
            })
            .collect::<Vec<_>>();
        entries_idx.reverse();

        {
            let tx = tx.clone();
            let zim = zim.clone();
            spawn("Feeder", move || {
                let mut entries_chunks = entries_idx
                    .par_chunks(entries_idx.len() / 128)
                    .collect::<Vec<_>>();
                entries_chunks.shuffle(&mut thread_rng());
                entries_chunks.into_par_iter().for_each(|chunck| {
                    chunck.iter().for_each(|i| {
                        let entry = zim.get_entry_bypath_index(*i).unwrap();
                        let item = entry.get_item(false).unwrap();
                        let size = item.get_size();
                        let blob = item.get_data_offset(size - 1, 1).unwrap();
                        dropper.dropout(blob.into());
                        dropper.dropout(item.into());
                        tx.send(entry).unwrap();
                    })
                })
            });
        }

        redirect_idx
            .into_iter()
            .map(|i| zim.get_entry_bypath_index(i).unwrap())
            .for_each(|e| {
                tx.send(e).unwrap();
            });
    });

    rx
}

impl Converter {
    pub fn new<P: AsRef<Path>>(
        zim: &Archive,
        outfile: P,
        concat_mode: ConcatMode,
        split_binary: bool,
    ) -> jbk::Result<Self> {
        let progress = Arc::new(ProgressBar::new(zim)?);
        let basic_creator = BasicCreator::new(
            &outfile,
            concat_mode,
            waj::VENDOR_ID,
            jbk::creator::Compression::zstd(),
            Arc::clone(&progress) as Arc<dyn jbk::creator::Progress>,
        )?;

        let binary_content_pack = if split_binary {
            let mut binary_content_path = outfile.as_ref().to_path_buf();
            binary_content_path.set_extension("binary.waj");
            let binary_content_file: Box<dyn PackRecipient> =
                AtomicOutFile::new(binary_content_path)?;
            let binary_content_pack = ContentPackCreator::new_from_output_with_progress(
                binary_content_file,
                jbk::PackId::from(2),
                waj::VENDOR_ID,
                Default::default(),
                jbk::creator::Compression::zstd(),
                Arc::clone(&progress) as Arc<dyn jbk::creator::Progress>,
            )?;
            Some(binary_content_pack)
        } else {
            None
        };

        let content_adder_switch = ContentAdderSwitch::new(basic_creator, binary_content_pack);

        let entry_store_creator = Box::new(waj::create::EntryStoreCreator::new(Some(
            zim.get_all_entrycount() as usize,
        )));

        Ok(Self {
            content_adder_switch,
            entry_store_creator,
            progress,
            has_main_page: false,
            dropper: Dropper::new(),
        })
    }

    fn finalize(self, outfile: &Path) -> jbk::Result<()> {
        let (basic_creator, extra_content_creators) = self.content_adder_switch.into_inner();
        basic_creator.finalize(outfile, self.entry_store_creator, extra_content_creators)
    }

    pub fn run(mut self, zim: Arc<Archive>, outfile: PathBuf) -> jbk::Result<()> {
        info!(
            "Converting zim file with {} entries",
            zim.get_all_entrycount()
        );

        let main_page = zim.get_mainentry().unwrap();

        let entry_input = entry_producer(zim, self.dropper.clone());

        while let Ok(e) = entry_input.recv() {
            self.handle(e)?;
        }

        if !self.has_main_page {
            let main_page_path = main_page.get_item(true).unwrap().get_path();
            let entry = ZimEntry::new_redirect("".into(), main_page_path, true);
            self.entry_store_creator.add_entry(&entry)?;
        }

        self.finalize(&outfile)
    }

    fn handle(&mut self, entry: zim_rs::entry::Entry) -> jbk::Result<()> {
        self.progress.entries.inc(1);

        let entry = ZimEntry::new(entry, &self.dropper, &mut self.content_adder_switch)?;
        if entry.is_main {
            self.has_main_page = true;
        }
        self.entry_store_creator.add_entry(&entry)
    }
}

fn main() -> jbk::Result<()> {
    let args = Cli::parse();

    let zim = Arc::new(Archive::new(args.zim_file.to_str().unwrap()).unwrap());
    let converter = Converter::new(&zim, &args.outfile, ConcatMode::OneFile, args.split)?;
    converter.run(zim, args.outfile)
}

use clap::Parser;

use anyhow::Result;
use ffmpeg::format::{input, Pixel};
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::frame::video::Video;
use ffmpeg_next as ffmpeg;
use image::buffer::ConvertBuffer;
use image::{imageops, GrayImage, ImageBuffer, RgbImage};
use std::ops::Deref;

/*
LICENSE
- uetchy <https://holodata.org> MIT
- Zhiming Wang <https://zhimingwang.org> WTFPL (https://github.com/zmwangx/rust-ffmpeg/blob/master/examples/dump-frames.rs)
*/

#[derive(Parser)]
struct Cli {
    #[clap(parse(from_os_str))]
    path: std::path::PathBuf,

    #[clap(short, long, default_value_t = 220)]
    width: u8,

    #[clap(short, long, default_value_t = 140)]
    height: u8,

    #[clap(long, default_value_t = 7_000_000)]
    threshold: u32,

    #[clap(long, default_value_t = 2)]
    match_threshold: u32,

    #[clap(long, default_value_t = 0.5)]
    scaling_factor: f32,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    ffmpeg::init()?;

    if let Ok(mut ictx) = input(&args.path) {
        let input = ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)?;

        let video_stream_index: usize = input.index();
        let fps = input.avg_frame_rate().0 as usize;
        println!("fps: {}", fps);

        let mut decoder = input.codec().decoder().video()?;
        let mut scaler = Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGB24,
            (decoder.width() as f32 * args.scaling_factor) as u32,
            (decoder.height() as f32 * args.scaling_factor) as u32,
            Flags::BILINEAR,
        )?;
        let mut frame_index: usize = 0;
        let mut consecutive_match_count: u32 = 0;
        let mut target: Option<usize> = None;
        let mut last_found_index: usize = 0;
        let mut cumsum: u32 = 0;

        let mut decoder_handler = |decoder: &mut ffmpeg::decoder::Video| -> Result<()> {
            let mut decoded = Video::empty();
            while decoder.receive_frame(&mut decoded).is_ok() {
                if frame_index % (10) == 0 {
                    let mut frame = Video::empty();
                    scaler.run(&decoded, &mut frame)?;

                    let mut img = into_ib(&frame).unwrap();

                    match target {
                        Some(fi) if fi <= frame_index => {
                            save_frame(&img, frame_index)?;
                            target = None;
                        }
                        _ => {}
                    }

                    let cropped = imageops::crop(&mut img, 0, 0, 220, 140);
                    //       save_frame(&cropped.to_image(), frame_index)?;

                    let gray: GrayImage = cropped.to_image().convert();
                    let ps: u32 = gray.into_iter().map(|x| *x as u32).sum();

                    let elapsed_frames = frame_index - last_found_index;

                    if ps >= args.threshold && elapsed_frames >= fps * 2 {
                        consecutive_match_count += 1;

                        if consecutive_match_count >= args.match_threshold {
                            let t_frame = frame_index + (fps * 4) as usize;
                            target = Some(t_frame);
                            let elapsed_seconds = elapsed_frames / fps;
                            let ts = t_frame / fps;
                            cumsum += 1;
                            println!(
                                "frame={} t_frame={} ts={} elapsed={} cumsum={}",
                                frame_index, t_frame, ts, elapsed_seconds, cumsum
                            );
                            consecutive_match_count = 0;
                            last_found_index = frame_index;
                        }
                    }
                }
                frame_index += 1;
            }
            Ok(())
        };

        for (stream, packet) in ictx.packets() {
            if stream.index() == video_stream_index {
                decoder.send_packet(&packet)?;
                decoder_handler(&mut decoder)?;
            }
        }
        decoder.send_eof()?;
        decoder_handler(&mut decoder)?;
    }

    Ok(())
}

fn into_ib(frame: &Video) -> Option<RgbImage> {
    ImageBuffer::from_raw(frame.width(), frame.height(), frame.data(0).to_vec())
}

fn save_frame<P, Container>(frame: &ImageBuffer<P, Container>, index: usize) -> Result<()>
where
    P: image::Pixel + 'static,
    [P::Subpixel]: image::EncodableLayout,
    Container: Deref<Target = [P::Subpixel]>,
{
    frame
        .save(format!("frames/{}.ppm", index))
        .expect("Failed to write frame");
    Ok(())
}

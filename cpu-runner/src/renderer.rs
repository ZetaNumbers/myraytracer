use crate::{winit, State};
use glam::{vec2, Vec2, Vec3, Vec4};
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use std::{num::NonZeroUsize, sync::Arc, thread, time};

pub struct Handle {
    thread: Option<thread::JoinHandle<()>>,
    continue_: Arc<()>,
}

impl Handle {
    pub fn new(state: Arc<State>) -> Self {
        let continue_ = Arc::new(());

        let thread = Some(thread::spawn({
            let continue_ = Arc::downgrade(&continue_);
            move || {
                let start = time::Instant::now();

                let size = state.window.inner_size();
                let size = winit::PhysicalSize {
                    width: size.width as usize,
                    height: size.height as usize,
                };

                log::info!(
                    "Starting a renderer thread for window size {}x{}",
                    size.width,
                    size.height
                );

                let shape = vec2(size.width as f32, size.height as f32);
                let pixel_shape = Vec2::ONE / shape;
                let viewport_shape = 2. * shape / shape.y;

                let update_time = time::Duration::from_secs_f64(1. / crate::UPDATE_RATE);
                let pixels_per_frame = NonZeroUsize::new({
                    let mut rng = rand_pcg::Pcg32::from_entropy();
                    let start = time::Instant::now();
                    let color = multi_sampled_color(
                        &state.world,
                        &mut rng,
                        Vec2::ZERO,
                        Vec2::ONE,
                        Vec2::splat(2.),
                    );
                    std::hint::black_box(color);
                    let elapsed = start.elapsed();
                    update_time.div_duration_f64(elapsed).floor() as usize
                })
                .unwrap_or(NonZeroUsize::new(1).unwrap());

                match (0..size.height).into_par_iter().try_for_each_init(
                    || {
                        (
                            rand_pcg::Pcg32::from_entropy(),
                            vec![[0; 4]; size.width].into_boxed_slice(),
                            pixels_per_frame.clone(),
                        )
                    },
                    |(ref mut rng, ref mut row_buffer, ref mut pixels_per_frame), row| {
                        let y = shape.y - row as f32 - 1.;
                        let mut column_range = 0..pixels_per_frame.get().min(size.width);
                        loop {
                            let start = time::Instant::now();
                            for (column, out) in
                                row_buffer[column_range.clone()].iter_mut().enumerate()
                            {
                                let xy = vec2(column as f32, y);
                                let uv = xy / shape;
                                *out = multi_sampled_color(
                                    &state.world,
                                    rng,
                                    uv,
                                    pixel_shape,
                                    viewport_shape,
                                );
                            }

                            let elapsed = start.elapsed();
                            *pixels_per_frame = NonZeroUsize::new(
                                (column_range.len() as f64 * update_time.div_duration_f64(elapsed))
                                    .floor() as usize,
                            )
                            .unwrap_or(NonZeroUsize::new(1).unwrap());

                            log::trace!("Flushing pixels at row {row}, columns {column_range:?}");
                            let mut pixels = state.pixels.lock();
                            if continue_.strong_count() == 0 {
                                return Err(RenderError::Cancel);
                            }
                            let frame = pixels.get_frame();
                            if frame.len() != size.width * size.height * 4 {
                                return Err(RenderError::Resize);
                            }
                            let (frame, _) = frame.as_chunks_mut::<4>();
                            let row_out = &mut frame[row * size.width..][..size.width];
                            row_out[column_range.clone()]
                                .copy_from_slice(&row_buffer[column_range.clone()]);

                            column_range =
                                column_range.end..column_range.end + pixels_per_frame.get();

                            if column_range.end > size.width {
                                return Ok(());
                            }
                        }
                    },
                ) {
                    Ok(()) => {
                        log::info!("Renderer thread finished in {:?}", start.elapsed());
                        state.window.request_redraw();
                    }
                    Err(RenderError::Cancel) => log::info!("Render cancelled"),
                    Err(RenderError::Resize) => {
                        log::warn!("Renderer thread detected a resize, cancelling render")
                    }
                }
            }
        }));
        Self { thread, continue_ }
    }

    pub fn restart(&mut self, state: Arc<State>) -> thread::Result<()> {
        self.break_join()?;
        *self = Handle::new(state);
        Ok(())
    }

    pub fn is_running(&mut self) -> thread::Result<bool> {
        if self.thread.is_some() {
            let is_running = Arc::weak_count(&self.continue_) != 0;

            if !is_running {
                self.thread.take().unwrap().join()?;
            }

            Ok(is_running)
        } else {
            Ok(false)
        }
    }

    pub fn break_join(&mut self) -> thread::Result<()> {
        if let Some(thread) = self.thread.take() {
            log::info!("Stopping thread {:?}", thread.thread());
            self.continue_ = Arc::new(());
            thread.join()?;
        }
        Ok(())
    }
}

enum RenderError {
    Cancel,
    Resize,
}

fn multi_sampled_color(
    world: &raytracer::World,
    rng: &mut rand_pcg::Pcg32,
    uv: Vec2,
    pixel_shape: Vec2,
    viewport_shape: Vec2,
) -> [u8; 4] {
    let sum = (0..crate::SAMPLES_PER_PIXEL)
        .map(|_| {
            let uv = uv + vec2(rng.gen(), rng.gen()) * pixel_shape;
            let ray = raytracer::Ray {
                origin: crate::ORIGIN,
                direction: crate::ORIGIN
                    + Vec3::from((
                        (uv - Vec2::splat(0.5)) * viewport_shape,
                        -crate::FOCAL_LENGTH,
                    )),
            };

            world
                .color(rng, ray, crate::MAX_DEPTH)
                .clamp(Vec4::ZERO, Vec4::ONE)
        })
        .reduce(|acc, c| acc + c)
        .unwrap_or(Vec4::ZERO);
    let avg = sum / crate::SAMPLES_PER_PIXEL as f32;
    linear_to_srgb(avg)
}

fn linear_to_srgb(color: Vec4) -> [u8; 4] {
    color.to_array().map(|c| {
        let s = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1. / 2.4) - 0.055
        };
        (s * 256.).clamp(0., 255.) as u8
    })
}

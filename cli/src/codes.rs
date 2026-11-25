use conversor_core::{ConversionError, DecodingError, DownloadError, OptionsError};

pub(crate) const SUCCESS: u8 = 0;
pub(crate) const OTHER_ERROR: u8 = 1;
pub(crate) const INVALID_ARGUMENTS: u8 = 2;
pub(crate) const INVALID_INPUT: u8 = 3;
pub(crate) const FFMPEG_FAILURE: u8 = 4;
pub(crate) const NOT_FITS: u8 = 5;
pub(crate) const TIMED_OUT: u8 = 6;
pub(crate) const DOWNLOAD_FAILURE: u8 = 7;
pub(crate) const CANCELLED: u8 = 130;

pub(crate) const CODES: [(u8, &str); 9] = [
    (SUCCESS, "éxito"),
    (
        OTHER_ERROR,
        "otro error (por ejemplo, no se pudo escribir la salida)",
    ),
    (INVALID_ARGUMENTS, "argumentos inválidos"),
    (
        INVALID_INPUT,
        "entrada inválida o ilegible (o video descargado demasiado grande)",
    ),
    (FFMPEG_FAILURE, "ffmpeg o ffprobe ausente o con fallo"),
    (NOT_FITS, "no cabe en el peso o en la duración del preset"),
    (TIMED_OUT, "ffmpeg, ffprobe o yt-dlp tardó demasiado"),
    (DOWNLOAD_FAILURE, "yt-dlp ausente o descarga fallida"),
    (CANCELLED, "cancelado con Ctrl-C"),
];

pub(crate) fn options(error: &OptionsError) -> u8 {
    match error {
        OptionsError::DoesNotFitEvenSpedUp { .. } => NOT_FITS,
        OptionsError::MaxSecondsOutsideOfRange { .. }
        | OptionsError::StartNotValid { .. }
        | OptionsError::FpsOutsideOfRange { .. }
        | OptionsError::FpsNotSupported { .. }
        | OptionsError::MaxMbNotSupported { .. }
        | OptionsError::MaxMbOutsideOfRange { .. }
        | OptionsError::StartOutsideOfVideo { .. } => INVALID_ARGUMENTS,
    }
}

pub(crate) fn conversion(error: &ConversionError) -> u8 {
    match error {
        ConversionError::Options(error) => options(error),
        ConversionError::Decoding(error) => decoding(error),
        ConversionError::DoesNotFitInSize { .. } => NOT_FITS,
        ConversionError::Cancelled => CANCELLED,
        ConversionError::Preview(_)
        | ConversionError::PreviewNotSaved(_)
        | ConversionError::SizeFitting(_) => OTHER_ERROR,
    }
}

pub(crate) fn download(error: &DownloadError) -> u8 {
    match error {
        DownloadError::LinkNotValid { .. } => INVALID_ARGUMENTS,
        DownloadError::YtDlpNotFound { .. }
        | DownloadError::ProcessNotLaunched { .. }
        | DownloadError::FailedDownload { .. }
        | DownloadError::FailedOwnDownload { .. }
        | DownloadError::FileNotLocated { .. } => DOWNLOAD_FAILURE,
        DownloadError::FileTooLarge { .. } => INVALID_INPUT,
        DownloadError::TimedOut { .. } => TIMED_OUT,
        DownloadError::Cancelled => CANCELLED,
        DownloadError::TempNotCreated(_) | DownloadError::ThreadNotLaunched { .. } => OTHER_ERROR,
    }
}

fn decoding(error: &DecodingError) -> u8 {
    match error {
        DecodingError::FileNotFound { .. }
        | DecodingError::NotRegularFile { .. }
        | DecodingError::UnreadableFile { .. }
        | DecodingError::FileTooLarge { .. }
        | DecodingError::WithoutVideoTrack
        | DecodingError::WithoutAudioTrack
        | DecodingError::AudioNotSupported { .. }
        | DecodingError::VideoCodecNotSupported { .. }
        | DecodingError::UnreadableDuration { .. }
        | DecodingError::DurationNotValid { .. }
        | DecodingError::DimensionsNotValid { .. }
        | DecodingError::VideoWithoutFrames
        | DecodingError::Clip(_) => INVALID_INPUT,
        DecodingError::FfmpegNotFound { .. }
        | DecodingError::ProcessNotLaunched { .. }
        | DecodingError::FfmpegFailed { .. }
        | DecodingError::UnreadableOutput { .. } => FFMPEG_FAILURE,
        DecodingError::TimedOut { .. } => TIMED_OUT,
        DecodingError::ThreadNotLaunched { .. } => OTHER_ERROR,
        DecodingError::Cancelled => CANCELLED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_code_appears_time_in_table() {
        for (index, (code, _)) in CODES.iter().enumerate() {
            assert!(
                CODES[..index].iter().all(|(other, _)| other != code),
                "código {code} repetido"
            );
        }
    }

    #[test]
    fn not_enters_nor_accelerated_is_not_fits_and_rest_of_options_arguments() {
        let not_enters = OptionsError::DoesNotFitEvenSpedUp {
            available_s: 25.0,
            max_s: 20.0,
            crop_s: 5.0,
            suggested_start_s: 5.0,
        };
        assert_eq!(options(&not_enters), NOT_FITS);
        assert_eq!(conversion(&ConversionError::Options(not_enters)), NOT_FITS);
        assert_eq!(
            options(&OptionsError::FpsOutsideOfRange { value: 60 }),
            INVALID_ARGUMENTS
        );
        assert_eq!(
            options(&OptionsError::FpsNotSupported {
                preset: "whatsapp-sticker",
                value: 24
            }),
            INVALID_ARGUMENTS
        );
    }

    #[test]
    fn failures_of_download_follow_exit_code_table() {
        let cases = [
            (
                DownloadError::LinkNotValid { reason: "x" },
                INVALID_ARGUMENTS,
            ),
            (
                DownloadError::YtDlpNotFound {
                    program: "yt-dlp".into(),
                },
                DOWNLOAD_FAILURE,
            ),
            (
                DownloadError::FailedDownload {
                    code: Some(1),
                    summarized_stderr: String::new(),
                },
                DOWNLOAD_FAILURE,
            ),
            (
                DownloadError::FileNotLocated { reason: "x" },
                DOWNLOAD_FAILURE,
            ),
            (DownloadError::FileTooLarge { max_bytes: 1 }, INVALID_INPUT),
            (
                DownloadError::TimedOut {
                    limit: std::time::Duration::from_secs(1),
                },
                TIMED_OUT,
            ),
            (DownloadError::Cancelled, CANCELLED),
            (
                DownloadError::TempNotCreated(std::io::Error::other("x")),
                OTHER_ERROR,
            ),
        ];
        for (error, code) in cases {
            assert_eq!(download(&error), code, "{error:?}");
        }
    }

    #[test]
    fn failures_of_decoding_split_by_cause() {
        let cases = [
            (DecodingError::WithoutVideoTrack, INVALID_INPUT),
            (
                DecodingError::FfmpegNotFound {
                    program: "ffmpeg".into(),
                },
                FFMPEG_FAILURE,
            ),
            (
                DecodingError::TimedOut {
                    program: "ffmpeg".into(),
                    limit: std::time::Duration::from_secs(1),
                },
                TIMED_OUT,
            ),
        ];
        for (error, code) in cases {
            assert_eq!(conversion(&error.into()), code);
        }
        assert_eq!(conversion(&DecodingError::Cancelled.into()), CANCELLED);
    }
}

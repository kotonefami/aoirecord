use std::io::BufWriter;

/// バッファリング中の Opus フレーム
struct PendingFrame {
    data: Vec<u8>,
    absgp: u64,
}

/// ユーザーごとの Opus トラック
pub struct Track {
    encoder: audiopus::coder::Encoder,
    ogg_writer: ogg::PacketWriter<'static, BufWriter<std::fs::File>>,
    packet_count: u64,
    pre_skip: u16,
    pending: Option<PendingFrame>,
}
impl Track {
    /// 新しい Opus トラックを作成します。
    pub fn create(path: std::path::PathBuf, bitrate: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let file = BufWriter::new(std::fs::File::create(path)?);
        let mut ogg_writer = ogg::PacketWriter::new(file);

        let mut encoder = audiopus::coder::Encoder::new(
            audiopus::SampleRate::Hz48000,
            audiopus::Channels::Stereo,
            audiopus::Application::Audio,
        )?;
        encoder.set_bitrate(audiopus::Bitrate::BitsPerSecond(bitrate as i32))?;
        let lookahead = encoder.lookahead()? as u16;

        // OpusHead パケット (19 bytes)
        let mut head = Vec::with_capacity(19);
        head.extend_from_slice(b"OpusHead");
        head.push(1);
        head.push(2);
        head.extend_from_slice(&lookahead.to_le_bytes());
        head.extend_from_slice(&48000u32.to_le_bytes());
        head.extend_from_slice(&0i16.to_le_bytes());
        head.push(0);

        ogg_writer.write_packet(
            head,
            1,
            ogg::PacketWriteEndInfo::EndPage,
            0,
        )?;

        // OpusTags パケット
        let vendor = b"aoirecord";
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor);
        tags.extend_from_slice(&0u32.to_le_bytes());

        ogg_writer.write_packet(
            tags,
            1,
            ogg::PacketWriteEndInfo::EndPage,
            0,
        )?;

        Ok(Self {
            encoder,
            ogg_writer,
            packet_count: 0,
            pre_skip: lookahead,
            pending: None,
        })
    }

    /// 20ms 分の PCM を Opus にエンコードして Ogg に書き込みます。
    ///
    /// 最終フレームはバッファリングされており、finalize() を呼ぶことで最終フレームも書き出されます。
    pub fn write_frame(&mut self, pcm: &[i16]) -> Result<(), Box<dyn std::error::Error>> {
        let mut output = vec![0u8; 4000];
        let len = self.encoder.encode(pcm, &mut output)?;
        output.truncate(len);

        let absgp = self.pre_skip as u64 + self.packet_count * 960;
        self.packet_count += 1;

        if let Some(prev) = self.pending.replace(PendingFrame { data: output, absgp }) {
            self.ogg_writer.write_packet(
                prev.data,
                1,
                ogg::PacketWriteEndInfo::NormalPacket,
                prev.absgp,
            )?;
        }
        Ok(())
    }

    /// 無音フレーム（ゼロ埋め）をエンコードして書き込みます。
    pub fn write_silent_frame(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.write_frame(&[0; 1920])
    }

    /// Ogg Opus ストリームを正しく閉じます。
    ///
    /// バッファリングしていた最終フレームに EOS フラグを付けて書き出します。
    pub fn finalize(mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(prev) = self.pending.take() {
            let absgp = self.pre_skip as u64 + self.packet_count * 960;
            self.ogg_writer.write_packet(
                prev.data,
                1,
                ogg::PacketWriteEndInfo::EndStream,
                absgp,
            )?;
        }
        Ok(())
    }
}

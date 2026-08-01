# frozen_string_literal: true

# 字形の輪郭を取り出すだけの、ごく小さな TrueType リーダ。
#
# 欲しいのは「異体字セレクタ付きの文字の輪郭」だけなので、必要な表しか読まない
# （head・maxp・loca・glyf・cmap）。汎用のフォントライブラリではない。
#
# 割り切っていること:
#   - cmap は書式 4・12（基本の対応表）と 14（異体字セレクタ）だけ
#   - 合成グリフは平行移動のみ（拡大・回転の指定は無視する）
#   - ヒンティングは読まない

class MiniTtf
  Glyph = Struct.new(:contours) # contours: [[[x, y], ...], ...]

  def initialize(path)
    @raw = File.binread(path)
    read_tables
    @head = read_head
    @loca = read_loca
    @cmap = nil
    @uvs = nil
  end

  attr_reader :units_per_em

  # 基本の対応表（符号位置 => グリフ番号）
  def cmap
    @cmap ||= read_cmap
  end

  # 異体字セレクタの表 {セレクタ => {基底 => グリフ番号}}
  def uvs
    @uvs ||= read_uvs
  end

  # `52FA_E0103` 形式から輪郭を得る。セレクタで引けなければ基底の字形を返す。
  def glyph_for_ivs(ivs)
    base, sel = ivs.split('_').map { |x| x.to_i(16) }
    gid = uvs.dig(sel, base) || cmap[base]
    gid ? glyph(gid) : nil
  end

  # グリフ番号から輪郭（閉じた折れ線の列）を得る。曲線は等分割で近似する。
  def glyph(gid, depth = 0)
    return nil if gid.nil? || depth > 4 || gid + 1 >= @loca.size

    from = @glyf_off + @loca[gid]
    return Glyph.new([]) if @loca[gid] == @loca[gid + 1]

    n = int16(from)
    n.negative? ? composite_glyph(from + 10, depth) : simple_glyph(from + 10, n)
  end

  private

  def read_tables
    num = uint16(4)
    @tables = {}
    num.times do |i|
      off = 12 + (i * 16)
      @tables[@raw[off, 4]] = [uint32(off + 8), uint32(off + 12)]
    end
    @glyf_off = @tables['glyf'][0]
  end

  def read_head
    off = @tables['head'][0]
    @units_per_em = uint16(off + 18)
    @index_to_loc = int16(off + 50)
  end

  def read_loca
    off, len = @tables['loca']
    if @index_to_loc.zero?
      (len / 2).times.map { |i| uint16(off + (i * 2)) * 2 }
    else
      (len / 4).times.map { |i| uint32(off + (i * 4)) }
    end
  end

  # --- cmap ---------------------------------------------------------------

  def cmap_subtables
    off = @tables['cmap'][0]
    uint16(off + 2).times.map do |i|
      rec = off + 4 + (i * 8)
      [uint16(rec), uint16(rec + 2), off + uint32(rec + 4)]
    end
  end

  def read_cmap
    best = cmap_subtables.find { |_, _, sub| uint16(sub) == 12 } ||
           cmap_subtables.find { |_, _, sub| uint16(sub) == 4 }
    return {} unless best

    sub = best[2]
    uint16(sub) == 12 ? read_cmap12(sub) : read_cmap4(sub)
  end

  def read_cmap12(sub)
    map = {}
    uint32(sub + 12).times do |i|
      g = sub + 16 + (i * 12)
      start, last, gid = uint32(g), uint32(g + 4), uint32(g + 8)
      (start..last).each_with_index { |c, k| map[c] = gid + k }
    end
    map
  end

  def read_cmap4(sub)
    seg = uint16(sub + 6) / 2
    ends = seg.times.map { |i| uint16(sub + 14 + (i * 2)) }
    starts = seg.times.map { |i| uint16(sub + 16 + (seg * 2) + (i * 2)) }
    deltas = seg.times.map { |i| int16(sub + 16 + (seg * 4) + (i * 2)) }
    range_off_pos = sub + 16 + (seg * 6)
    map = {}
    seg.times do |i|
      ro = uint16(range_off_pos + (i * 2))
      (starts[i]..ends[i]).each do |c|
        next if c == 0xFFFF

        gid = if ro.zero?
                (c + deltas[i]) & 0xFFFF
              else
                g = uint16(range_off_pos + (i * 2) + ro + ((c - starts[i]) * 2))
                g.zero? ? 0 : (g + deltas[i]) & 0xFFFF
              end
        map[c] = gid unless gid.zero?
      end
    end
    map
  end

  # 書式 14。既定の異体字（defaultUVS）は基底と同じ字形なので表に入れない。
  def read_uvs
    sub = cmap_subtables.find { |_, _, s| uint16(s) == 14 }&.last
    return {} unless sub

    out = {}
    uint32(sub + 6).times do |i|
      rec = sub + 10 + (i * 11)
      selector = uint24(rec)
      non_default = uint32(rec + 7)
      next if non_default.zero?

      base = sub + non_default
      table = out[selector] ||= {}
      uint32(base).times do |k|
        m = base + 4 + (k * 5)
        table[uint24(m)] = uint16(m + 3)
      end
    end
    out
  end

  # --- glyf ---------------------------------------------------------------

  def simple_glyph(pos, num_contours)
    ends = num_contours.times.map { |i| uint16(pos + (i * 2)) }
    pos += num_contours * 2
    pos += 2 + uint16(pos) # 命令列は読み飛ばす
    total = ends.empty? ? 0 : ends.last + 1

    flags = []
    while flags.size < total
      f = byte(pos)
      pos += 1
      flags << f
      if f.anybits?(0x08) # 繰り返し
        n = byte(pos)
        pos += 1
        n.times { flags << f }
      end
    end
    xs, pos = read_coords(pos, flags, total, 0x02, 0x10)
    ys, = read_coords(pos, flags, total, 0x04, 0x20)

    contours = []
    start = 0
    ends.each do |last|
      pts = (start..last).map { |i| [xs[i], ys[i], flags[i].anybits?(0x01)] }
      contours << flatten_quadratic(pts) unless pts.empty?
      start = last + 1
    end
    Glyph.new(contours)
  end

  def read_coords(pos, flags, total, short_bit, same_bit)
    vals = []
    v = 0
    total.times do |i|
      f = flags[i]
      if f.anybits?(short_bit)
        d = byte(pos)
        pos += 1
        v += f.anybits?(same_bit) ? d : -d
      elsif !f.anybits?(same_bit)
        v += int16(pos)
        pos += 2
      end
      vals << v
    end
    [vals, pos]
  end

  # 2 次ベジエ。制御点が続くときは中点が曲線上の点になる規則を補う。
  def flatten_quadratic(pts)
    return [] if pts.empty?

    # 曲線上の点から始まるように回す
    start = pts.index { |p| p[2] } || 0
    pts = pts.rotate(start)
    pts << pts.first
    out = [[pts[0][0], pts[0][1]]]
    i = 1
    while i < pts.size
      x, y, on = pts[i]
      if on
        out << [x, y]
        i += 1
        next
      end
      nx, ny, non = pts[(i + 1) % pts.size]
      unless non # 制御点が続く → 中点が曲線上
        nx = (x + nx) / 2.0
        ny = (y + ny) / 2.0
      end
      p0 = out.last
      1.upto(6) do |k|
        t = k / 6.0
        out << [(1 - t)**2 * p0[0] + 2 * (1 - t) * t * x + t * t * nx,
                (1 - t)**2 * p0[1] + 2 * (1 - t) * t * y + t * t * ny]
      end
      i += non ? 2 : 1
    end
    out
  end

  def composite_glyph(pos, depth)
    contours = []
    loop do
      flags = uint16(pos)
      gid = uint16(pos + 2)
      pos += 4
      if flags.anybits?(0x0001) # 引数が 2 バイト
        dx = int16(pos)
        dy = int16(pos + 2)
        pos += 4
      else
        dx = int8(pos)
        dy = int8(pos + 1)
        pos += 2
      end
      # 拡大・回転の指定は読み飛ばす（この用途では平行移動で足りる）
      pos += 2 if flags.anybits?(0x0008)
      pos += 4 if flags.anybits?(0x0040)
      pos += 8 if flags.anybits?(0x0080)
      unless flags.anybits?(0x0002) # 引数が点番号なら扱えない
        dx = dy = 0
      end
      sub = glyph(gid, depth + 1)
      contours.concat(sub.contours.map { |c| c.map { |x, y| [x + dx, y + dy] } }) if sub
      break unless flags.anybits?(0x0020) # 続きがある
    end
    Glyph.new(contours)
  end

  # --- 数値の読み出し -----------------------------------------------------

  def byte(i) = @raw.getbyte(i)
  def uint16(i) = @raw[i, 2].unpack1('n')
  def int16(i) = @raw[i, 2].unpack1('s>')
  def int8(i) = @raw[i, 1].unpack1('c')
  def uint24(i) = (byte(i) << 16) | (byte(i + 1) << 8) | byte(i + 2)
  def uint32(i) = @raw[i, 4].unpack1('N')
end

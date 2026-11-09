import AppKit
import Foundation

guard CommandLine.arguments.count == 3,
      let bundle = Bundle(path: CommandLine.arguments[1]),
      let name = bundle.object(forInfoDictionaryKey: "CFBundleIconName") as? String,
      let image = bundle.image(forResource: name),
      let source = NSImage(contentsOfFile: CommandLine.arguments[2]) else {
    fatalError("The built app's configured icon or selected source artwork cannot be loaded.")
}

func pixels(_ image: NSImage) -> [UInt8] {
    let size = 1024
    var rectangle = CGRect(x: 0, y: 0, width: size, height: size)
    guard let cgImage = image.cgImage(forProposedRect: &rectangle, context: nil, hints: nil) else {
        fatalError("The app icon cannot be decoded.")
    }
    var bytes = [UInt8](repeating: 0, count: size * size * 4)
    bytes.withUnsafeMutableBytes { buffer in
        guard let context = CGContext(data: buffer.baseAddress, width: size, height: size,
                                      bitsPerComponent: 8, bytesPerRow: size * 4,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
            fatalError("The app icon comparison could not be initialized.")
        }
        context.draw(cgImage, in: rectangle)
    }
    return bytes
}

guard pixels(image) == pixels(source) else {
    fatalError("The built app's configured icon does not match the selected artwork.")
}
print("built app icon matches the selected artwork")

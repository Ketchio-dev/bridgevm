import Foundation
import Network

// The async bridge over NWConnection's callback API, used by
// UnixSocketNDJSONTransport. Split out of VirtualMachineClient.swift, which is
// well past any size the ratchet tolerates elsewhere.

extension NWConnection {
  func startAndWait() async throws {
    let resume = ContinuationResumeBox<Void>()

    stateUpdateHandler = { state in
      switch state {
      case .ready:
        resume.succeed(())
      case .failed(let error):
        resume.fail(error)
      case .cancelled:
        resume.fail(DaemonTransportError.connectionFailed)
      case .waiting(let error):
        // A missing Unix socket yields .waiting, never .failed, and it does not
        // recover: measured still waiting past four seconds, and creating the
        // socket afterwards did not make it ready. Without this the caller pays
        // the whole request timeout to learn the daemon is not running.
        resume.fail(error)
      default:
        break
      }
    }

    start(queue: .global(qos: .userInitiated))

    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        resume.set(continuation)
      }
    } onCancel: {
      cancel()
      resume.fail(DaemonTransportError.connectionFailed)
    }
  }

  func sendAndWait(_ data: Data) async throws {
    let resume = ContinuationResumeBox<Void>()

    try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
        resume.set(continuation)
        send(
          content: data,
          completion: .contentProcessed { error in
            if let error {
              resume.fail(error)
            } else {
              resume.succeed(())
            }
          })
      }
    } onCancel: {
      cancel()
      resume.fail(DaemonTransportError.connectionFailed)
    }
  }

  func receiveLine() async throws -> Data {
    var accumulator = NDJSONLineAccumulator()

    while true {
      let chunk = try await receiveChunk()

      if chunk.isEmpty {
        throw DaemonTransportError.responseClosed
      }

      if let line = try accumulator.append(chunk) { return line }
    }
  }

  private func receiveChunk() async throws -> Data {
    let resume = ContinuationResumeBox<Data>()

    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        resume.set(continuation)
        receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { data, _, isComplete, error in
          if let error {
            resume.fail(error)
          } else if let data, !data.isEmpty {
            resume.succeed(data)
          } else if isComplete {
            resume.succeed(Data())
          } else {
            resume.fail(DaemonTransportError.responseEncodingInvalid)
          }
        }
      }
    } onCancel: {
      cancel()
      resume.fail(DaemonTransportError.connectionFailed)
    }
  }
}

import AppleVzRunnerCore
import Foundation

exit(AppleVzRunnerCommand.run(arguments: Array(CommandLine.arguments.dropFirst())))

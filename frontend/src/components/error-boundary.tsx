import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";

interface Props {
	children: ReactNode;
}

interface State {
	hasError: boolean;
	error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
	state: State = { hasError: false, error: null };

	static getDerivedStateFromError(error: Error): State {
		return { hasError: true, error };
	}

	componentDidCatch(error: Error, info: ErrorInfo) {
		console.error("ErrorBoundary caught:", error, info.componentStack);
	}

	render() {
		if (this.state.hasError) {
			return (
				<div className="flex flex-col items-center justify-center gap-4 py-16">
					<div className="rounded-lg border border-destructive/30 bg-destructive/5 p-6 text-center">
						<h2 className="text-lg font-semibold text-foreground">
							Something went wrong
						</h2>
						<p className="mt-1 text-sm text-muted-foreground">
							{this.state.error?.message ?? "An unexpected error occurred."}
						</p>
						<Button
							className="mt-4"
							onClick={() => this.setState({ hasError: false, error: null })}
						>
							Try again
						</Button>
					</div>
				</div>
			);
		}
		return this.props.children;
	}
}

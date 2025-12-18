declare module 'plotly.js-dist-min' {
	interface PlotData {
		x: number[];
		y: number[];
		mode?: 'markers' | 'lines' | 'lines+markers' | 'text' | 'none';
		type?: 'scatter' | 'bar' | 'scatter3d' | 'surface' | 'heatmap';
		marker?: {
			size?: number | number[];
			color?: string | number | number[];
			colorscale?: string | [number, string][];
			opacity?: number;
		};
		text?: string[];
		hoverinfo?: 'text' | 'x' | 'y' | 'x+y' | 'all' | 'none' | 'skip';
		hoverlabel?: {
			bgcolor?: string;
			bordercolor?: string;
			font?: { color?: string; size?: number };
		};
		name?: string;
	}

	interface Layout {
		title?: string | { text?: string; font?: { color?: string; size?: number } };
		paper_bgcolor?: string;
		plot_bgcolor?: string;
		xaxis?: {
			title?: string;
			color?: string;
			gridcolor?: string;
			zerolinecolor?: string;
		};
		yaxis?: {
			title?: string;
			color?: string;
			gridcolor?: string;
			zerolinecolor?: string;
		};
		margin?: { l?: number; r?: number; t?: number; b?: number };
		hovermode?: 'closest' | 'x' | 'y' | 'x unified' | 'y unified' | false;
		showlegend?: boolean;
	}

	interface Config {
		responsive?: boolean;
		displayModeBar?: boolean;
		modeBarButtonsToRemove?: string[];
		displaylogo?: boolean;
	}

	export function newPlot(
		container: HTMLElement,
		data: Partial<PlotData>[],
		layout?: Partial<Layout>,
		config?: Partial<Config>
	): Promise<void>;

	export function purge(container: HTMLElement): void;
}
